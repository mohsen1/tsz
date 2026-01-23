//! Source map tests - Part 4 (JSX, utility types, remaining tests)

use crate::emit_context::EmitContext;
use crate::emitter::{Printer, PrinterOptions, ScriptTarget};
use crate::lowering_pass::LoweringPass;
use crate::parser::ParserState;
#[allow(unused_imports)]
use crate::source_map::*;
#[allow(unused_imports)]
use crate::source_map_test_utils::{decode_mappings, find_line_col, has_mapping_for_prefixes};
use serde_json::Value;

#[test]
fn test_source_map_jsx_es5_spread_attributes() {
    // Test JSX spread attributes transformation
    let source = r#"const props = { id: "main", className: "container" };
const element = <div {...props} data-testid="test">Content</div>;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
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
        output.contains("props"),
        "expected props reference in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX spread attributes"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_self_closing() {
    // Test self-closing JSX elements
    let source = r#"const input = <input type="text" placeholder="Enter name" />;
const img = <img src="photo.jpg" alt="Photo" />;
const br = <br />;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
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
        output.contains("input") && output.contains("img"),
        "expected self-closing elements in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for self-closing JSX"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_nested_elements() {
    // Test nested JSX elements transformation
    let source = r#"const nested = (
    <div className="outer">
        <header>
            <h1>Title</h1>
            <nav>
                <ul>
                    <li>Home</li>
                    <li>About</li>
                </ul>
            </nav>
        </header>
        <main>
            <section>Content</section>
        </main>
    </div>
);"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
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
        output.contains("header") && output.contains("nav") && output.contains("main"),
        "expected nested elements in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested JSX"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_expressions() {
    // Test JSX with embedded expressions
    let source = r#"const name = "World";
const count = 42;
const items = ["a", "b", "c"];
const element = (
    <div>
        <h1>Hello, {name}!</h1>
        <p>Count: {count}</p>
        <ul>{items.map(function(item) { return <li key={item}>{item}</li>; })}</ul>
        <span>{count > 10 ? "Many" : "Few"}</span>
    </div>
);"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
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
        output.contains("name") && output.contains("count") && output.contains("items"),
        "expected expression references in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_component_props() {
    // Test JSX component with various prop patterns
    let source = r#"function Button(props: { onClick: () => void; disabled?: boolean; children: any }) {
    return <button onClick={props.onClick} disabled={props.disabled}>{props.children}</button>;
}

function Icon(props: { name: string; size?: number }) {
    return <span className={"icon-" + props.name} style={{ fontSize: props.size || 16 }} />;
}

const app = (
    <div>
        <Button onClick={function() { console.log("clicked"); }} disabled={false}>
            <Icon name="star" size={24} />
            Click Me
        </Button>
    </div>
);"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
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
        output.contains("Button") && output.contains("Icon"),
        "expected component names in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX components"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_event_handlers() {
    // Test JSX event handlers with various patterns
    let source = r#"var state = { count: 0 };

function handleClick() {
    state.count++;
}

function handleInput(e: any) {
    console.log(e.target.value);
}

const form = (
    <form onSubmit={function(e) { e.preventDefault(); }}>
        <input type="text" onChange={handleInput} onBlur={function() { console.log("blur"); }} />
        <button type="button" onClick={handleClick}>Increment</button>
        <button type="submit">Submit</button>
    </form>
);"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
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
        output.contains("handleClick") && output.contains("handleInput"),
        "expected event handler references in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX event handlers"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_conditional_rendering() {
    // Test JSX conditional rendering patterns
    let source = r#"var isLoggedIn = true;
var hasPermission = false;
var items: string[] = [];

const element = (
    <div>
        {isLoggedIn ? <span>Welcome</span> : <span>Please login</span>}
        {hasPermission && <button>Admin</button>}
        {items.length > 0 ? (
            <ul>
                {items.map(function(item) { return <li>{item}</li>; })}
            </ul>
        ) : (
            <p>No items</p>
        )}
    </div>
);"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
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
        output.contains("isLoggedIn") && output.contains("hasPermission"),
        "expected conditional variables in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional JSX"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_jsx_es5_comprehensive() {
    // Comprehensive JSX ES5 transform test
    let source = r#"// Component definitions
function Header(props: { title: string; subtitle?: string }) {
    return (
        <header className="header">
            <h1>{props.title}</h1>
            {props.subtitle && <h2>{props.subtitle}</h2>}
        </header>
    );
}

function Card(props: { children: any; className?: string }) {
    var baseClass = "card";
    return <div className={baseClass + " " + (props.className || "")}>{props.children}</div>;
}

function List(props: { items: string[]; onSelect: (item: string) => void }) {
    return (
        <ul className="list">
            {props.items.map(function(item, index) {
                return (
                    <li key={index} onClick={function() { props.onSelect(item); }}>
                        {item}
                    </li>
                );
            })}
        </ul>
    );
}

// Main app
var appState = {
    title: "My App",
    items: ["Item 1", "Item 2", "Item 3"],
    selectedItem: null as string | null
};

function handleSelect(item: string) {
    appState.selectedItem = item;
    console.log("Selected:", item);
}

const App = (
    <div className="app">
        <Header title={appState.title} subtitle="Welcome" />
        <main>
            <Card className="content">
                <List items={appState.items} onSelect={handleSelect} />
                {appState.selectedItem && (
                    <p>Selected: {appState.selectedItem}</p>
                )}
            </Card>
        </main>
        <footer>
            <p>&copy; 2024</p>
        </footer>
    </div>
);"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
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
        output.contains("Header"),
        "expected Header component in output. output: {output}"
    );
    assert!(
        output.contains("Card"),
        "expected Card component in output. output: {output}"
    );
    assert!(
        output.contains("List"),
        "expected List component in output. output: {output}"
    );
    assert!(
        output.contains("handleSelect"),
        "expected handleSelect function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive JSX"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Class Accessor ES5 Transform Source Map Tests
// =============================================================================
// Tests for class accessor (getter/setter) compilation with ES5 target.
// Accessors should transform to Object.defineProperty calls while preserving
// source map accuracy.

#[test]
fn test_source_map_accessor_es5_basic_getter_setter() {
    // Test basic getter/setter transformation
    let source = r#"class Person {
    private _name: string = "";

    get name(): string {
        return this._name;
    }

    set name(value: string) {
        this._name = value;
    }
}

const person = new Person();
person.name = "Alice";
console.log(person.name);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected Person class in output. output: {output}"
    );
    assert!(
        output.contains("name"),
        "expected name accessor in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for getter/setter"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_accessor_es5_static_accessors() {
    // Test static getter/setter transformation
    let source = r#"class Counter {
    private static _count: number = 0;

    static get count(): number {
        return Counter._count;
    }

    static set count(value: number) {
        Counter._count = value;
    }

    static increment(): void {
        Counter.count++;
    }
}

Counter.count = 10;
Counter.increment();
console.log(Counter.count);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("count"),
        "expected count accessor in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static accessors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

#[test]
fn test_source_map_loop_es5_for_of_destructuring() {
    // Test for-of with destructuring transformation
    let source = r#"const pairs = [[1, "one"], [2, "two"], [3, "three"]];

for (const [num, name] of pairs) {
    console.log(num, name);
}

const entries = [{ id: 1, value: "a" }, { id: 2, value: "b" }];

for (const { id, value } of entries) {
    console.log(id, value);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("pairs") && output.contains("entries"),
        "expected pairs and entries arrays in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_in_destructuring() {
    // Test for-in with object property access
    let source = r#"const config = {
    host: "localhost",
    port: 8080,
    debug: true
};

const settings: { [key: string]: any } = {};

for (const prop in config) {
    if (config.hasOwnProperty(prop)) {
        settings[prop] = config[prop as keyof typeof config];
    }
}

console.log(settings);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("config") && output.contains("settings"),
        "expected config and settings in output. output: {output}"
    );
    assert!(
        output.contains("hasOwnProperty"),
        "expected hasOwnProperty call in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-in destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_of_string() {
    // Test for-of with string iteration
    let source = r#"const message = "Hello";
const chars: string[] = [];

for (const char of message) {
    chars.push(char.toUpperCase());
}

console.log(chars.join("-"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("message") && output.contains("chars"),
        "expected message and chars in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of string"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_of_map_set() {
    // Test for-of with Map and Set iteration
    let source = r#"const map = new Map<string, number>();
map.set("a", 1);
map.set("b", 2);

for (const [key, value] of map) {
    console.log(key, value);
}

const set = new Set<number>([1, 2, 3]);

for (const item of set) {
    console.log(item);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Map") && output.contains("Set"),
        "expected Map and Set in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of Map/Set"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_nested_for_of() {
    // Test nested for-of loops
    let source = r#"const matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
let total = 0;

for (const row of matrix) {
    for (const cell of row) {
        total += cell;
    }
}

const nested = [["a", "b"], ["c", "d"]];

for (const outer of nested) {
    for (const inner of outer) {
        console.log(inner);
    }
}

console.log(total);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("matrix") && output.contains("nested"),
        "expected matrix and nested arrays in output. output: {output}"
    );
    assert!(
        output.contains("total"),
        "expected total variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested for-of"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_of_break_continue() {
    // Test for-of with break and continue
    let source = r#"const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let result: number[] = [];

for (const num of numbers) {
    if (num === 3) {
        continue;
    }
    if (num > 7) {
        break;
    }
    result.push(num);
}

outer: for (const x of [1, 2, 3]) {
    for (const y of [4, 5, 6]) {
        if (x === 2 && y === 5) {
            break outer;
        }
        console.log(x, y);
    }
}

console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("numbers") && output.contains("result"),
        "expected numbers and result in output. output: {output}"
    );
    assert!(
        output.contains("break") || output.contains("continue"),
        "expected break/continue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of break/continue"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_for_of_iterator() {
    // Test for-of with custom iterator
    let source = r#"class Range {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator]() {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }
}

const range = new Range(1, 5);

for (const num of range) {
    console.log(num);
}

function* generateNumbers() {
    yield 1;
    yield 2;
    yield 3;
}

for (const n of generateNumbers()) {
    console.log(n);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Range"),
        "expected Range class in output. output: {output}"
    );
    assert!(
        output.contains("generateNumbers"),
        "expected generateNumbers function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of iterator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_loop_es5_comprehensive() {
    // Comprehensive for-of/for-in test
    let source = r#"// Data structures
interface User {
    id: number;
    name: string;
    roles: string[];
}

const users: User[] = [
    { id: 1, name: "Alice", roles: ["admin", "user"] },
    { id: 2, name: "Bob", roles: ["user"] }
];

// for-of with object destructuring
for (const { id, name, roles } of users) {
    console.log("User:", id, name);

    // Nested for-of with array
    for (const role of roles) {
        console.log("  Role:", role);
    }
}

// for-in with property enumeration
const config: { [key: string]: any } = {
    host: "localhost",
    port: 8080,
    debug: true,
    timeout: 5000
};

const configCopy: { [key: string]: any } = {};

for (const key in config) {
    if (Object.prototype.hasOwnProperty.call(config, key)) {
        configCopy[key] = config[key];
    }
}

// for-of with index tracking
const items = ["a", "b", "c"];
let index = 0;

for (const item of items) {
    console.log(index, item);
    index++;
}

// for-of with Map entries
const userMap = new Map<number, string>();
userMap.set(1, "Alice");
userMap.set(2, "Bob");

for (const [userId, userName] of userMap.entries()) {
    console.log("Map entry:", userId, userName);
}

// Labeled for-of with break
search: for (const user of users) {
    for (const role of user.roles) {
        if (role === "admin") {
            console.log("Found admin:", user.name);
            break search;
        }
    }
}

console.log("Config copy:", configCopy);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("users"),
        "expected users array in output. output: {output}"
    );
    assert!(
        output.contains("config"),
        "expected config object in output. output: {output}"
    );
    assert!(
        output.contains("userMap"),
        "expected userMap in output. output: {output}"
    );
    assert!(
        output.contains("hasOwnProperty"),
        "expected hasOwnProperty check in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive for-of/for-in"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Async/Await Transform ES5 Source Map Tests
// =============================================================================
// Tests for async/await compilation with ES5 target focusing on:
// try/catch in async, Promise chain transforms, async arrow functions.

#[test]
fn test_source_map_async_es5_try_catch_basic() {
    // Test basic try/catch in async function
    let source = r#"async function fetchData(url: string): Promise<string> {
    try {
        const response = await fetch(url);
        const data = await response.text();
        return data;
    } catch (error) {
        console.error("Fetch failed:", error);
        return "";
    }
}

fetchData("https://api.example.com/data");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData"),
        "expected fetchData function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async try/catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_try_catch_finally() {
    // Test try/catch/finally in async function
    let source = r#"async function processWithCleanup(): Promise<void> {
    let resource: any = null;
    try {
        resource = await acquireResource();
        await processResource(resource);
    } catch (error) {
        console.error("Processing failed:", error);
        throw error;
    } finally {
        if (resource) {
            await releaseResource(resource);
        }
        console.log("Cleanup complete");
    }
}

async function acquireResource() {
    return { id: 1 };
}

async function processResource(r: any) {
    console.log("Processing", r);
}

async function releaseResource(r: any) {
    console.log("Releasing", r);
}

processWithCleanup();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processWithCleanup"),
        "expected processWithCleanup function in output. output: {output}"
    );
    assert!(
        output.contains("acquireResource") && output.contains("releaseResource"),
        "expected resource functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async try/catch/finally"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_promise_chain() {
    // Test Promise chain transforms
    let source = r#"function fetchUser(id: number): Promise<{ name: string }> {
    return Promise.resolve({ name: "User" + id });
}

function fetchPosts(userId: number): Promise<string[]> {
    return Promise.resolve(["Post 1", "Post 2"]);
}

async function getUserWithPosts(id: number) {
    const user = await fetchUser(id);
    const posts = await fetchPosts(id);
    return { user, posts };
}

// Promise chain equivalent
function getUserWithPostsChain(id: number) {
    return fetchUser(id)
        .then(function(user) {
            return fetchPosts(id).then(function(posts) {
                return { user: user, posts: posts };
            });
        })
        .catch(function(error) {
            console.error(error);
            return null;
        });
}

getUserWithPosts(1);
getUserWithPostsChain(2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchUser") && output.contains("fetchPosts"),
        "expected fetch functions in output. output: {output}"
    );
    assert!(
        output.contains("getUserWithPosts"),
        "expected getUserWithPosts function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Promise chain"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_arrow_function() {
    // Test async arrow functions
    let source = r#"const fetchData = async (url: string): Promise<any> => {
    const response = await fetch(url);
    return response.json();
};

const processItems = async (items: number[]): Promise<number[]> => {
    const results: number[] = [];
    for (const item of items) {
        const processed = await processItem(item);
        results.push(processed);
    }
    return results;
};

const processItem = async (item: number): Promise<number> => item * 2;

const shortAsync = async () => "done";

const asyncWithDefault = async (value: number = 10) => value * 2;

fetchData("https://api.example.com");
processItems([1, 2, 3]);
shortAsync();
asyncWithDefault();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData") && output.contains("processItems"),
        "expected async arrow functions in output. output: {output}"
    );
    assert!(
        output.contains("shortAsync"),
        "expected shortAsync in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async arrow functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_class_methods() {
    // Test async class methods
    let source = r#"class ApiClient {
    private baseUrl: string;

    constructor(baseUrl: string) {
        this.baseUrl = baseUrl;
    }

    async get(endpoint: string): Promise<any> {
        try {
            const response = await fetch(this.baseUrl + endpoint);
            return await response.json();
        } catch (error) {
            console.error("GET failed:", error);
            throw error;
        }
    }

    async post(endpoint: string, data: any): Promise<any> {
        try {
            const response = await fetch(this.baseUrl + endpoint, {
                method: "POST",
                body: JSON.stringify(data)
            });
            return await response.json();
        } catch (error) {
            console.error("POST failed:", error);
            throw error;
        }
    }

    static async create(url: string): Promise<ApiClient> {
        return new ApiClient(url);
    }
}

const client = new ApiClient("https://api.example.com");
client.get("/users");
client.post("/users", { name: "Alice" });
ApiClient.create("https://api.example.com");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ApiClient"),
        "expected ApiClient class in output. output: {output}"
    );
    assert!(
        output.contains("get") && output.contains("post"),
        "expected get/post methods in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async class methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_iife() {
    // Test async IIFE (Immediately Invoked Function Expression)
    let source = r#"(async function() {
    console.log("Starting async IIFE");
    const result = await Promise.resolve("done");
    console.log("Result:", result);
})();

(async () => {
    const data = await fetchData();
    console.log("Data:", data);
})();

async function fetchData() {
    return { value: 42 };
}

const asyncResult = (async () => {
    return await Promise.all([
        Promise.resolve(1),
        Promise.resolve(2),
        Promise.resolve(3)
    ]);
})();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData"),
        "expected fetchData function in output. output: {output}"
    );
    assert!(
        output.contains("asyncResult"),
        "expected asyncResult variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async IIFE"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_nested_try_catch() {
    // Test nested try/catch in async functions
    let source = r#"async function complexOperation(): Promise<string> {
    try {
        const first = await step1();
        try {
            const second = await step2(first);
            try {
                const third = await step3(second);
                return third;
            } catch (innerError) {
                console.error("Step 3 failed:", innerError);
                return await fallback3();
            }
        } catch (middleError) {
            console.error("Step 2 failed:", middleError);
            return await fallback2();
        }
    } catch (outerError) {
        console.error("Step 1 failed:", outerError);
        return await fallback1();
    }
}

async function step1() { return "step1"; }
async function step2(input: string) { return input + "-step2"; }
async function step3(input: string) { return input + "-step3"; }
async function fallback1() { return "fallback1"; }
async function fallback2() { return "fallback2"; }
async function fallback3() { return "fallback3"; }

complexOperation();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("complexOperation"),
        "expected complexOperation function in output. output: {output}"
    );
    assert!(
        output.contains("step1") && output.contains("step2") && output.contains("step3"),
        "expected step functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested try/catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_parallel_await() {
    // Test parallel await patterns with Promise.all/race
    let source = r#"async function fetchAllData(): Promise<[any, any, any]> {
    const [users, posts, comments] = await Promise.all([
        fetchUsers(),
        fetchPosts(),
        fetchComments()
    ]);
    return [users, posts, comments];
}

async function fetchFirstResponse(): Promise<any> {
    try {
        const result = await Promise.race([
            fetchFast(),
            fetchSlow(),
            timeout(5000)
        ]);
        return result;
    } catch (error) {
        return null;
    }
}

async function fetchUsers() { return [{ id: 1 }]; }
async function fetchPosts() { return [{ id: 1 }]; }
async function fetchComments() { return [{ id: 1 }]; }
async function fetchFast() { return "fast"; }
async function fetchSlow() { return "slow"; }
function timeout(ms: number): Promise<never> {
    return new Promise(function(_, reject) {
        setTimeout(function() { reject(new Error("Timeout")); }, ms);
    });
}

fetchAllData();
fetchFirstResponse();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchAllData"),
        "expected fetchAllData function in output. output: {output}"
    );
    assert!(
        output.contains("Promise"),
        "expected Promise in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for parallel await"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_error_rethrow() {
    // Test error rethrowing patterns in async
    let source = r#"class CustomError extends Error {
    constructor(message: string, public code: number) {
        super(message);
        this.name = "CustomError";
    }
}

async function processWithRetry(maxRetries: number): Promise<string> {
    let lastError: Error | null = null;

    for (let i = 0; i < maxRetries; i++) {
        try {
            return await attemptOperation();
        } catch (error) {
            lastError = error as Error;
            console.log("Attempt " + (i + 1) + " failed, retrying...");
        }
    }

    throw lastError || new Error("All retries failed");
}

async function attemptOperation(): Promise<string> {
    if (Math.random() < 0.5) {
        throw new CustomError("Random failure", 500);
    }
    return "success";
}

async function wrapError(): Promise<void> {
    try {
        await processWithRetry(3);
    } catch (error) {
        throw new CustomError("Wrapped: " + (error as Error).message, 400);
    }
}

processWithRetry(3);
wrapError();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("CustomError"),
        "expected CustomError class in output. output: {output}"
    );
    assert!(
        output.contains("processWithRetry"),
        "expected processWithRetry function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for error rethrow"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_es5_transform_comprehensive() {
    // Comprehensive async/await transform test
    let source = r#"// Types
interface ApiResponse<T> {
    data: T;
    status: number;
}

// Async arrow with type parameters
const fetchTyped = async <T>(url: string): Promise<ApiResponse<T>> => {
    try {
        const response = await fetch(url);
        const data = await response.json();
        return { data: data as T, status: response.status };
    } catch (error) {
        throw new Error("Fetch failed: " + (error as Error).message);
    }
};

// Class with async methods and try/catch
class DataService {
    private cache: Map<string, any> = new Map();

    async fetchWithCache(key: string): Promise<any> {
        if (this.cache.has(key)) {
            return this.cache.get(key);
        }

        try {
            const data = await this.fetchRemote(key);
            this.cache.set(key, data);
            return data;
        } catch (error) {
            console.error("Cache miss and fetch failed:", error);
            throw error;
        }
    }

    private async fetchRemote(key: string): Promise<any> {
        return { key: key, value: "data" };
    }

    async batchFetch(keys: string[]): Promise<any[]> {
        const promises = keys.map(async (key) => {
            try {
                return await this.fetchWithCache(key);
            } catch {
                return null;
            }
        });
        return Promise.all(promises);
    }
}

// Async generator simulation with try/catch
async function* asyncGenerator(): AsyncGenerator<number> {
    for (let i = 0; i < 5; i++) {
        try {
            yield await Promise.resolve(i);
        } catch {
            yield -1;
        }
    }
}

// IIFE with complex async flow
const initApp = (async () => {
    try {
        console.log("Initializing...");
        const service = new DataService();
        const data = await service.batchFetch(["a", "b", "c"]);
        console.log("Data loaded:", data);
        return { success: true, data: data };
    } catch (error) {
        console.error("Init failed:", error);
        return { success: false, error: error };
    } finally {
        console.log("Init complete");
    }
})();

// Usage
fetchTyped<{ name: string }>("/api/user");
const service = new DataService();
service.fetchWithCache("test");
initApp.then(function(result) { console.log(result); });"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchTyped"),
        "expected fetchTyped function in output. output: {output}"
    );
    assert!(
        output.contains("DataService"),
        "expected DataService class in output. output: {output}"
    );
    assert!(
        output.contains("fetchWithCache"),
        "expected fetchWithCache method in output. output: {output}"
    );
    assert!(
        output.contains("batchFetch"),
        "expected batchFetch method in output. output: {output}"
    );
    assert!(
        output.contains("initApp"),
        "expected initApp variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async/await"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Generator Transform ES5 Source Map Tests
// =============================================================================
// Tests for generator function compilation with ES5 target focusing on:
// yield expressions, generator delegation (yield*).

#[test]
fn test_source_map_generator_es5_basic_yield() {
    // Test basic yield expressions
    let source = r#"function* simpleGenerator() {
    yield 1;
    yield 2;
    yield 3;
}

const gen = simpleGenerator();
console.log(gen.next().value);
console.log(gen.next().value);
console.log(gen.next().value);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("simpleGenerator"),
        "expected simpleGenerator function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic yield"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_yield_with_values() {
    // Test yield with computed values
    let source = r#"function* valueGenerator(start: number) {
    let current = start;
    while (current < start + 5) {
        yield current * 2;
        current++;
    }
}

function* expressionYield() {
    const a = 10;
    const b = 20;
    yield a + b;
    yield a * b;
    yield Math.max(a, b);
}

const gen1 = valueGenerator(1);
const gen2 = expressionYield();

for (const val of gen1) {
    console.log(val);
}

console.log(gen2.next().value);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("valueGenerator") && output.contains("expressionYield"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for yield with values"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_delegation() {
    // Test generator delegation (yield*)
    let source = r#"function* innerGenerator() {
    yield "a";
    yield "b";
    yield "c";
}

function* anotherInner() {
    yield 1;
    yield 2;
}

function* outerGenerator() {
    yield "start";
    yield* innerGenerator();
    yield "middle";
    yield* anotherInner();
    yield "end";
}

const gen = outerGenerator();
for (const value of gen) {
    console.log(value);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("innerGenerator") && output.contains("outerGenerator"),
        "expected generator functions in output. output: {output}"
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

#[test]
fn test_source_map_generator_es5_return_value() {
    // Test generator with return values
    let source = r#"function* generatorWithReturn(): Generator<number, string, unknown> {
    yield 1;
    yield 2;
    yield 3;
    return "done";
}

function* conditionalReturn(shouldStop: boolean): Generator<number, string, unknown> {
    yield 1;
    if (shouldStop) {
        return "stopped early";
    }
    yield 2;
    yield 3;
    return "completed";
}

const gen1 = generatorWithReturn();
let result = gen1.next();
while (!result.done) {
    console.log("Value:", result.value);
    result = gen1.next();
}
console.log("Return:", result.value);

const gen2 = conditionalReturn(true);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("generatorWithReturn") && output.contains("conditionalReturn"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator return"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_try_catch() {
    // Test generator with try/catch
    let source = r#"function* safeGenerator(): Generator<number, void, unknown> {
    try {
        yield 1;
        yield 2;
        throw new Error("Generator error");
    } catch (error) {
        console.error("Caught:", error);
        yield -1;
    } finally {
        console.log("Generator cleanup");
    }
}

function* generatorWithFinally(): Generator<string, void, unknown> {
    try {
        yield "start";
        yield "middle";
    } finally {
        yield "cleanup";
    }
}

const gen1 = safeGenerator();
for (const val of gen1) {
    console.log(val);
}

const gen2 = generatorWithFinally();
console.log(gen2.next().value);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("safeGenerator") && output.contains("generatorWithFinally"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator try/catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_infinite() {
    // Test infinite generator patterns
    let source = r#"function* infiniteCounter(): Generator<number, never, unknown> {
    let count = 0;
    while (true) {
        yield count++;
    }
}

function* fibonacci(): Generator<number, never, unknown> {
    let a = 0;
    let b = 1;
    while (true) {
        yield a;
        const temp = a;
        a = b;
        b = temp + b;
    }
}

function* idGenerator(prefix: string): Generator<string, never, unknown> {
    let id = 0;
    while (true) {
        yield prefix + "-" + (id++);
    }
}

const counter = infiniteCounter();
console.log(counter.next().value);
console.log(counter.next().value);

const fib = fibonacci();
for (let i = 0; i < 10; i++) {
    console.log(fib.next().value);
}

const ids = idGenerator("user");
console.log(ids.next().value);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("infiniteCounter") && output.contains("fibonacci"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        output.contains("idGenerator"),
        "expected idGenerator function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for infinite generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_class_iterator() {
    // Test generator implementing iterator protocol
    let source = r#"class Range {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator](): Generator<number, void, unknown> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }
}

class Tree<T> {
    constructor(
        public value: T,
        public left?: Tree<T>,
        public right?: Tree<T>
    ) {}

    *inOrder(): Generator<T, void, unknown> {
        if (this.left) {
            yield* this.left.inOrder();
        }
        yield this.value;
        if (this.right) {
            yield* this.right.inOrder();
        }
    }
}

const range = new Range(1, 5);
for (const num of range) {
    console.log(num);
}

const tree = new Tree(2, new Tree(1), new Tree(3));
for (const val of tree.inOrder()) {
    console.log(val);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Range") && output.contains("Tree"),
        "expected Range and Tree classes in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for iterator protocol"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_class_methods() {
    // Test generator methods in classes
    let source = r#"class DataProcessor {
    private data: number[] = [];

    constructor(data: number[]) {
        this.data = data;
    }

    *processAll(): Generator<number, void, unknown> {
        for (const item of this.data) {
            yield this.process(item);
        }
    }

    *processFiltered(predicate: (n: number) => boolean): Generator<number, void, unknown> {
        for (const item of this.data) {
            if (predicate(item)) {
                yield this.process(item);
            }
        }
    }

    private process(item: number): number {
        return item * 2;
    }

    static *range(start: number, end: number): Generator<number, void, unknown> {
        for (let i = start; i <= end; i++) {
            yield i;
        }
    }
}

const processor = new DataProcessor([1, 2, 3, 4, 5]);
for (const result of processor.processAll()) {
    console.log(result);
}

for (const result of processor.processFiltered(function(n) { return n > 2; })) {
    console.log(result);
}

for (const n of DataProcessor.range(1, 10)) {
    console.log(n);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected DataProcessor class in output. output: {output}"
    );
    assert!(
        output.contains("processAll") && output.contains("processFiltered"),
        "expected generator methods in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class generator methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_async_generator() {
    // Test async generator patterns
    let source = r#"async function* asyncDataFetcher(urls: string[]): AsyncGenerator<any, void, unknown> {
    for (const url of urls) {
        const data = await fetch(url).then(function(r) { return r.json(); });
        yield data;
    }
}

async function* timedGenerator(interval: number): AsyncGenerator<number, void, unknown> {
    let count = 0;
    while (count < 5) {
        await new Promise(function(resolve) { setTimeout(resolve, interval); });
        yield count++;
    }
}

async function* paginatedFetch(baseUrl: string): AsyncGenerator<any[], void, unknown> {
    let page = 1;
    let hasMore = true;

    while (hasMore) {
        const response = await fetch(baseUrl + "?page=" + page);
        const data = await response.json();
        yield data.items;
        hasMore = data.hasMore;
        page++;
    }
}

async function processAsync() {
    const fetcher = asyncDataFetcher(["url1", "url2"]);
    for await (const data of fetcher) {
        console.log(data);
    }
}

processAsync();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("asyncDataFetcher") && output.contains("timedGenerator"),
        "expected async generator functions in output. output: {output}"
    );
    assert!(
        output.contains("paginatedFetch"),
        "expected paginatedFetch function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async generators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_transform_comprehensive() {
    // Comprehensive generator transform test
    let source = r#"// Utility generators
function* take<T>(iterable: Iterable<T>, count: number): Generator<T, void, unknown> {
    let i = 0;
    for (const item of iterable) {
        if (i >= count) return;
        yield item;
        i++;
    }
}

function* map<T, U>(iterable: Iterable<T>, fn: (item: T) => U): Generator<U, void, unknown> {
    for (const item of iterable) {
        yield fn(item);
    }
}

function* filter<T>(iterable: Iterable<T>, predicate: (item: T) => boolean): Generator<T, void, unknown> {
    for (const item of iterable) {
        if (predicate(item)) {
            yield item;
        }
    }
}

// Complex generator with delegation
function* pipeline<T>(
    source: Iterable<T>,
    ...transforms: ((items: Iterable<T>) => Iterable<T>)[]
): Generator<T, void, unknown> {
    let current: Iterable<T> = source;
    for (const transform of transforms) {
        current = transform(current);
    }
    yield* current as Generator<T>;
}

// Class with generator methods
class LazyCollection<T> {
    constructor(private items: T[]) {}

    *[Symbol.iterator](): Generator<T, void, unknown> {
        yield* this.items;
    }

    *map<U>(fn: (item: T) => U): Generator<U, void, unknown> {
        for (const item of this.items) {
            yield fn(item);
        }
    }

    *filter(predicate: (item: T) => boolean): Generator<T, void, unknown> {
        for (const item of this.items) {
            if (predicate(item)) {
                yield item;
            }
        }
    }

    *flatMap<U>(fn: (item: T) => Iterable<U>): Generator<U, void, unknown> {
        for (const item of this.items) {
            yield* fn(item);
        }
    }
}

// Usage
const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

// Using take
for (const n of take(numbers, 3)) {
    console.log("Take:", n);
}

// Using map
for (const n of map(numbers, function(x) { return x * 2; })) {
    console.log("Map:", n);
}

// Using filter
for (const n of filter(numbers, function(x) { return x % 2 === 0; })) {
    console.log("Filter:", n);
}

// Using LazyCollection
const collection = new LazyCollection(numbers);
for (const n of collection.filter(function(x) { return x > 5; })) {
    console.log("Collection:", n);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("take"),
        "expected take function in output. output: {output}"
    );
    assert!(
        output.contains("map"),
        "expected map function in output. output: {output}"
    );
    assert!(
        output.contains("filter"),
        "expected filter function in output. output: {output}"
    );
    assert!(
        output.contains("LazyCollection"),
        "expected LazyCollection class in output. output: {output}"
    );
    assert!(
        output.contains("pipeline"),
        "expected pipeline function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Destructuring Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_destructuring_es5_array_basic() {
    let source = r#"const numbers = [1, 2, 3, 4, 5];
const [first, second, third] = numbers;
console.log(first, second, third);

const [a, b, c, d, e] = [10, 20, 30, 40, 50];
console.log(a + b + c + d + e);

let [x, y] = [100, 200];
[x, y] = [y, x];
console.log(x, y);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("first"),
        "expected first variable in output. output: {output}"
    );
    assert!(
        output.contains("second"),
        "expected second variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_object_basic() {
    let source = r#"const person = { name: "Alice", age: 30, city: "NYC" };
const { name, age, city } = person;
console.log(name, age, city);

const { name: personName, age: personAge } = person;
console.log(personName, personAge);

const config = { host: "localhost", port: 8080 };
const { host, port } = config;
console.log(host + ":" + port);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("name"),
        "expected name property in output. output: {output}"
    );
    assert!(
        output.contains("personName"),
        "expected personName alias in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_nested_array() {
    let source = r#"const matrix = [[1, 2], [3, 4], [5, 6]];
const [[a, b], [c, d], [e, f]] = matrix;
console.log(a, b, c, d, e, f);

const deep = [[[1, 2]], [[3, 4]]];
const [[[x, y]], [[z, w]]] = deep;
console.log(x + y + z + w);

const mixed = [1, [2, [3, [4]]]];
const [one, [two, [three, [four]]]] = mixed;
console.log(one, two, three, four);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("matrix"),
        "expected matrix variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested array destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_nested_object() {
    let source = r#"const user = {
    profile: {
        name: "Bob",
        address: {
            city: "Boston",
            zip: "02101"
        }
    },
    settings: {
        theme: "dark",
        notifications: true
    }
};

const { profile: { name, address: { city, zip } } } = user;
console.log(name, city, zip);

const { settings: { theme, notifications } } = user;
console.log(theme, notifications);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("user"),
        "expected user variable in output. output: {output}"
    );
    assert!(
        output.contains("profile"),
        "expected profile property in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested object destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_mixed() {
    let source = r#"const data = {
    users: [
        { id: 1, name: "Alice" },
        { id: 2, name: "Bob" }
    ],
    metadata: {
        count: 2,
        tags: ["admin", "user"]
    }
};

const { users: [first, second], metadata: { count, tags: [tag1, tag2] } } = data;
console.log(first.name, second.name, count, tag1, tag2);

const response = { items: [[1, 2], [3, 4]], status: { code: 200 } };
const { items: [[a, b], [c, d]], status: { code } } = response;
console.log(a, b, c, d, code);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("users"),
        "expected users array in output. output: {output}"
    );
    assert!(
        output.contains("metadata"),
        "expected metadata object in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mixed destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_defaults() {
    let source = r#"const [a = 1, b = 2, c = 3] = [10];
console.log(a, b, c);

const { name = "Unknown", age = 0, active = true } = { name: "Alice" };
console.log(name, age, active);

const { config: { timeout = 5000, retries = 3 } = {} } = {};
console.log(timeout, retries);

function process({ value = 0, label = "default" } = {}) {
    return label + ": " + value;
}
console.log(process({ value: 42 }));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("process"),
        "expected process function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for destructuring with defaults"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_function_params() {
    let source = r#"function processUser({ name, age, email }: { name: string; age: number; email: string }) {
    console.log(name, age, email);
}

function processCoords([x, y, z]: number[]) {
    return x + y + z;
}

const greet = ({ firstName, lastName }: { firstName: string; lastName: string }) => {
    return "Hello, " + firstName + " " + lastName;
};

class Handler {
    handle({ type, payload }: { type: string; payload: any }) {
        console.log(type, payload);
    }
}

processUser({ name: "Alice", age: 30, email: "alice@example.com" });
console.log(processCoords([1, 2, 3]));
console.log(greet({ firstName: "John", lastName: "Doe" }));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processUser"),
        "expected processUser function in output. output: {output}"
    );
    assert!(
        output.contains("processCoords"),
        "expected processCoords function in output. output: {output}"
    );
    assert!(
        output.contains("greet"),
        "expected greet function in output. output: {output}"
    );
    assert!(
        output.contains("Handler"),
        "expected Handler class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function parameter destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_rest_patterns() {
    let source = r#"const [first, second, ...rest] = [1, 2, 3, 4, 5];
console.log(first, second, rest);

const { name, ...others } = { name: "Alice", age: 30, city: "NYC" };
console.log(name, others);

function processItems([head, ...tail]: number[]) {
    console.log("Head:", head);
    console.log("Tail:", tail);
    return tail.reduce((a, b) => a + b, head);
}

const { a, b, ...remaining } = { a: 1, b: 2, c: 3, d: 4, e: 5 };
console.log(a, b, remaining);

console.log(processItems([10, 20, 30, 40]));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("rest"),
        "expected rest variable in output. output: {output}"
    );
    assert!(
        output.contains("processItems"),
        "expected processItems function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest pattern destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_loop_patterns() {
    let source = r#"const pairs = [[1, "one"], [2, "two"], [3, "three"]];
for (const [num, word] of pairs) {
    console.log(num + ": " + word);
}

const users = [
    { id: 1, name: "Alice" },
    { id: 2, name: "Bob" },
    { id: 3, name: "Charlie" }
];
for (const { id, name } of users) {
    console.log(id + " - " + name);
}

const entries = new Map([["a", 1], ["b", 2], ["c", 3]]);
for (const [key, value] of entries) {
    console.log(key + " => " + value);
}

const matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
for (const [a, b, c] of matrix) {
    console.log(a + b + c);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("pairs"),
        "expected pairs array in output. output: {output}"
    );
    assert!(
        output.contains("users"),
        "expected users array in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for loop destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_es5_comprehensive() {
    let source = r#"// Comprehensive destructuring patterns for ES5 transform testing

// Basic array and object destructuring
const [x, y, z] = [1, 2, 3];
const { name, age } = { name: "Alice", age: 30 };

// Nested destructuring
const {
    user: {
        profile: { firstName, lastName },
        settings: { theme }
    }
} = {
    user: {
        profile: { firstName: "John", lastName: "Doe" },
        settings: { theme: "dark" }
    }
};

// Destructuring with defaults
const [a = 1, b = 2] = [10];
const { timeout = 5000, retries = 3 } = {};

// Rest patterns
const [head, ...tail] = [1, 2, 3, 4, 5];
const { id, ...metadata } = { id: 1, type: "user", active: true };

// Function parameter destructuring
function processConfig({
    server: { host = "localhost", port = 8080 },
    database: { name: dbName, pool = 10 }
}: {
    server: { host?: string; port?: number };
    database: { name: string; pool?: number };
}) {
    return host + ":" + port + " -> " + dbName + " (pool: " + pool + ")";
}

// Arrow function with destructuring
const formatUser = ({ name, email }: { name: string; email: string }) =>
    name + " <" + email + ">";

// Class with destructuring in methods
class DataProcessor {
    private data: any[];

    constructor(data: any[]) {
        this.data = data;
    }

    process() {
        return this.data.map(({ value, label }) => label + ": " + value);
    }

    *iterate() {
        for (const { id, ...rest } of this.data) {
            yield { id, processed: true, ...rest };
        }
    }
}

// Destructuring in loops
const pairs: [string, number][] = [["a", 1], ["b", 2], ["c", 3]];
for (const [key, val] of pairs) {
    console.log(key + " = " + val);
}

// Complex nested array/object mix
const response = {
    data: {
        users: [
            { id: 1, info: { name: "Alice", scores: [90, 85, 92] } },
            { id: 2, info: { name: "Bob", scores: [88, 91, 87] } }
        ]
    },
    meta: { total: 2, page: 1 }
};

const {
    data: { users: [{ info: { name: user1Name, scores: [score1] } }] },
    meta: { total }
} = response;

// Swap using destructuring
let m = 1, n = 2;
[m, n] = [n, m];

// Computed property names with destructuring
const key = "dynamicKey";
const { [key]: dynamicValue } = { dynamicKey: "found!" };

// Usage
console.log(x, y, z, name, age);
console.log(firstName, lastName, theme);
console.log(a, b, timeout, retries);
console.log(head, tail, id, metadata);
console.log(processConfig({
    server: { host: "api.example.com" },
    database: { name: "mydb" }
}));
console.log(formatUser({ name: "Test", email: "test@example.com" }));

const processor = new DataProcessor([
    { id: 1, value: 10, label: "A" },
    { id: 2, value: 20, label: "B" }
]);
console.log(processor.process());

console.log(user1Name, score1, total);
console.log(m, n);
console.log(dynamicValue);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processConfig"),
        "expected processConfig function in output. output: {output}"
    );
    assert!(
        output.contains("formatUser"),
        "expected formatUser function in output. output: {output}"
    );
    assert!(
        output.contains("DataProcessor"),
        "expected DataProcessor class in output. output: {output}"
    );
    assert!(
        output.contains("dynamicValue"),
        "expected dynamicValue variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Spread/Rest Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_spread_rest_es5_array_spread_basic() {
    let source = r#"const arr1 = [1, 2, 3];
const arr2 = [4, 5, 6];
const combined = [...arr1, ...arr2];
console.log(combined);

const numbers = [10, 20, 30];
const expanded = [...numbers];
console.log(expanded);

const nested = [[1, 2], [3, 4]];
const flat = [...nested[0], ...nested[1]];
console.log(flat);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("arr1"),
        "expected arr1 in output. output: {output}"
    );
    assert!(
        output.contains("combined"),
        "expected combined in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array spread"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_object_spread_basic() {
    let source = r#"const obj1 = { a: 1, b: 2 };
const obj2 = { c: 3, d: 4 };
const merged = { ...obj1, ...obj2 };
console.log(merged);

const defaults = { theme: "light", lang: "en" };
const settings = { ...defaults, theme: "dark" };
console.log(settings);

const person = { name: "Alice", age: 30 };
const updated = { ...person, age: 31 };
console.log(updated);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("obj1"),
        "expected obj1 in output. output: {output}"
    );
    assert!(
        output.contains("merged"),
        "expected merged in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object spread"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_function_call_spread() {
    let source = r#"function sum(a: number, b: number, c: number): number {
    return a + b + c;
}

const args = [1, 2, 3] as const;
const result = sum(...args);
console.log(result);

function log(...items: any[]): void {
    console.log(...items);
}

const messages = ["hello", "world"];
log(...messages);

Math.max(...[1, 5, 3, 9, 2]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("sum"),
        "expected sum function in output. output: {output}"
    );
    assert!(
        output.contains("log"),
        "expected log function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function call spread"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_array_with_elements() {
    let source = r#"const middle = [3, 4, 5];
const full = [1, 2, ...middle, 6, 7];
console.log(full);

const prefix = [0];
const suffix = [9, 10];
const range = [...prefix, 1, 2, ...middle, 8, ...suffix];
console.log(range);

const items = ["b", "c"];
const alphabet = ["a", ...items, "d", "e"];
console.log(alphabet);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("middle"),
        "expected middle in output. output: {output}"
    );
    assert!(
        output.contains("full"),
        "expected full in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array spread with elements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_object_with_properties() {
    let source = r#"const base = { x: 1, y: 2 };
const extended = { z: 3, ...base, w: 4 };
console.log(extended);

const config = { debug: false };
const options = { verbose: true, ...config, timeout: 5000 };
console.log(options);

const user = { name: "Bob" };
const profile = { id: 1, ...user, role: "admin", ...{ active: true } };
console.log(profile);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("base"),
        "expected base in output. output: {output}"
    );
    assert!(
        output.contains("extended"),
        "expected extended in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object spread with properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_rest_parameters() {
    let source = r#"function collect(...items: number[]): number[] {
    return items;
}

function processAll(first: string, ...rest: string[]): string {
    return first + ": " + rest.join(", ");
}

const formatItems = (prefix: string, ...values: any[]) => {
    return prefix + values.map(v => String(v)).join("");
};

class Handler {
    handle(action: string, ...args: any[]): void {
        console.log(action, args);
    }
}

console.log(collect(1, 2, 3, 4, 5));
console.log(processAll("Items", "a", "b", "c"));
console.log(formatItems("Values: ", 1, 2, 3));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("collect"),
        "expected collect function in output. output: {output}"
    );
    assert!(
        output.contains("processAll"),
        "expected processAll function in output. output: {output}"
    );
    assert!(
        output.contains("Handler"),
        "expected Handler class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_array_rest_elements() {
    let source = r#"const [first, ...remaining] = [1, 2, 3, 4, 5];
console.log(first, remaining);

const [head, second, ...tail] = [10, 20, 30, 40, 50];
console.log(head, second, tail);

function processArray([a, b, ...rest]: number[]): void {
    console.log(a, b, rest);
}

const numbers = [100, 200, 300, 400];
const [x, ...others] = numbers;
console.log(x, others);

processArray([1, 2, 3, 4, 5]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("first"),
        "expected first in output. output: {output}"
    );
    assert!(
        output.contains("remaining"),
        "expected remaining in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array rest elements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_object_rest_properties() {
    let source = r#"const { name, ...rest } = { name: "Alice", age: 30, city: "NYC" };
console.log(name, rest);

const { id, type, ...metadata } = { id: 1, type: "user", active: true, role: "admin" };
console.log(id, type, metadata);

function extractUser({ username, ...props }: { username: string; [key: string]: any }): void {
    console.log(username, props);
}

const config = { host: "localhost", port: 8080, debug: true };
const { host, ...serverConfig } = config;
console.log(host, serverConfig);

extractUser({ username: "bob", email: "bob@example.com", verified: true });"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("name"),
        "expected name in output. output: {output}"
    );
    assert!(
        output.contains("rest"),
        "expected rest in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object rest properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_nested_patterns() {
    let source = r#"const data = {
    users: [
        { id: 1, name: "Alice", scores: [90, 85, 92] },
        { id: 2, name: "Bob", scores: [88, 91, 87] }
    ],
    metadata: { count: 2, page: 1 }
};

const { users: [first, ...otherUsers], ...restData } = data;
console.log(first, otherUsers, restData);

const nested = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
const [[a, ...row1Rest], ...otherRows] = nested;
console.log(a, row1Rest, otherRows);

function process({ items: [head, ...tail], ...options }: { items: number[]; [key: string]: any }) {
    console.log(head, tail, options);
}

process({ items: [1, 2, 3], debug: true, verbose: false });"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("data"),
        "expected data in output. output: {output}"
    );
    assert!(
        output.contains("nested"),
        "expected nested in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested spread/rest patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_comprehensive() {
    let source = r#"// Comprehensive spread/rest patterns for ES5 transform testing

// Array spread
const arr1 = [1, 2, 3];
const arr2 = [4, 5, 6];
const combined = [...arr1, ...arr2];
const withElements = [0, ...arr1, 10, ...arr2, 20];

// Object spread
const obj1 = { a: 1, b: 2 };
const obj2 = { c: 3, d: 4 };
const merged = { ...obj1, ...obj2 };
const withProps = { prefix: "start", ...obj1, middle: true, ...obj2, suffix: "end" };

// Function call spread
function sum(...nums: number[]): number {
    return nums.reduce((a, b) => a + b, 0);
}
const numbers = [1, 2, 3, 4, 5];
const total = sum(...numbers);

// Rest parameters with different positions
function processArgs(first: string, second: number, ...rest: any[]): void {
    console.log(first, second, rest);
}

// Array rest elements
const [head, ...tail] = [1, 2, 3, 4, 5];
const [a, b, ...remaining] = numbers;

// Object rest properties
const { name, age, ...metadata } = { name: "Alice", age: 30, city: "NYC", country: "USA" };

// Nested patterns
const data = {
    users: [{ id: 1, ...obj1 }, { id: 2, ...obj2 }],
    settings: { ...merged, extra: true }
};
const { users: [firstUser, ...otherUsers], ...restData } = data;

// Class with spread/rest
class DataCollector {
    private items: any[];

    constructor(...initialItems: any[]) {
        this.items = [...initialItems];
    }

    add(...newItems: any[]): void {
        this.items = [...this.items, ...newItems];
    }

    getAll(): any[] {
        return [...this.items];
    }

    extract(): { first: any; rest: any[] } {
        const [first, ...rest] = this.items;
        return { first, rest };
    }
}

// Arrow functions with rest
const collectRest = (...items: any[]) => [...items];
const processRest = (first: any, ...rest: any[]) => ({ first, rest });

// Spread in new expression
class Point {
    constructor(public x: number, public y: number, public z?: number) {}
}
const coords = [10, 20, 30] as const;
const point = new Point(...coords);

// Usage
console.log(combined, withElements);
console.log(merged, withProps);
console.log(total);
processArgs("hello", 42, true, "extra", 123);
console.log(head, tail, a, b, remaining);
console.log(name, age, metadata);
console.log(firstUser, otherUsers, restData);

const collector = new DataCollector(1, 2, 3);
collector.add(4, 5);
console.log(collector.getAll());
console.log(collector.extract());

console.log(collectRest(1, 2, 3));
console.log(processRest("first", "a", "b", "c"));
console.log(point);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("sum"),
        "expected sum function in output. output: {output}"
    );
    assert!(
        output.contains("processArgs"),
        "expected processArgs function in output. output: {output}"
    );
    assert!(
        output.contains("DataCollector"),
        "expected DataCollector class in output. output: {output}"
    );
    assert!(
        output.contains("Point"),
        "expected Point class in output. output: {output}"
    );
    assert!(
        output.contains("collectRest"),
        "expected collectRest function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive spread/rest"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Class Expression Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_class_expr_es5_anonymous() {
    let source = r#"const MyClass = class {
    constructor(public value: number) {}

    getValue(): number {
        return this.value;
    }
};

const instance = new MyClass(42);
console.log(instance.getValue());

const factory = function() {
    return class {
        name: string = "default";
        greet() { return "Hello, " + this.name; }
    };
};

const Created = factory();
const obj = new Created();
console.log(obj.greet());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("MyClass"),
        "expected MyClass in output. output: {output}"
    );
    assert!(
        output.contains("getValue"),
        "expected getValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for anonymous class expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_named() {
    let source = r##"const MyClass = class NamedClass {
    static className = "NamedClass";

    constructor(public id: number) {}

    describe(): string {
        return NamedClass.className + "#" + this.id;
    }
};

const instance = new MyClass(1);
console.log(instance.describe());

const Container = class InnerClass {
    static create() {
        return new InnerClass();
    }

    value = 100;
};

const created = Container.create();
console.log(created.value);"##;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("MyClass"),
        "expected MyClass in output. output: {output}"
    );
    assert!(
        output.contains("describe"),
        "expected describe method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for named class expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_in_return() {
    let source = r#"function createCounter(initial: number) {
    return class {
        private count: number = initial;

        increment(): number {
            return ++this.count;
        }

        decrement(): number {
            return --this.count;
        }

        getCount(): number {
            return this.count;
        }
    };
}

const Counter = createCounter(10);
const counter = new Counter();
console.log(counter.increment());
console.log(counter.decrement());

function makeLogger(prefix: string) {
    return class Logger {
        log(message: string): void {
            console.log(prefix + ": " + message);
        }
    };
}

const PrefixedLogger = makeLogger("[INFO]");
const logger = new PrefixedLogger();
logger.log("Hello");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createCounter"),
        "expected createCounter in output. output: {output}"
    );
    assert!(
        output.contains("increment"),
        "expected increment method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression in return"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_with_extends() {
    let source = r#"class Base {
    constructor(public name: string) {}

    greet(): string {
        return "Hello, " + this.name;
    }
}

const Extended = class extends Base {
    constructor(name: string, public age: number) {
        super(name);
    }

    greet(): string {
        return super.greet() + ", age " + this.age;
    }
};

const instance = new Extended("Alice", 30);
console.log(instance.greet());

function createDerived(base: typeof Base) {
    return class extends base {
        extra = "additional";

        getExtra(): string {
            return this.extra;
        }
    };
}

const Derived = createDerived(Base);
const derived = new Derived("Bob");
console.log(derived.greet());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Base"),
        "expected Base class in output. output: {output}"
    );
    assert!(
        output.contains("Extended"),
        "expected Extended in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression with extends"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_with_static() {
    let source = r#"const Singleton = class {
    private static instance: any;
    private value: number;

    private constructor(value: number) {
        this.value = value;
    }

    static getInstance(): any {
        if (!Singleton.instance) {
            Singleton.instance = new Singleton(42);
        }
        return Singleton.instance;
    }

    getValue(): number {
        return this.value;
    }
};

const Registry = class {
    static items: Map<string, any> = new Map();

    static register(key: string, value: any): void {
        Registry.items.set(key, value);
    }

    static get(key: string): any {
        return Registry.items.get(key);
    }
};

Registry.register("test", { data: 123 });
console.log(Registry.get("test"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Singleton"),
        "expected Singleton in output. output: {output}"
    );
    assert!(
        output.contains("Registry"),
        "expected Registry in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression with static"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_in_variable() {
    let source = r#"let DynamicClass = class {
    value = 1;
};

DynamicClass = class {
    value = 2;
    double() { return this.value * 2; }
};

const instance = new DynamicClass();
console.log(instance.double());

var ReassignableClass = class First {
    type = "first";
};

ReassignableClass = class Second {
    type = "second";
    getType() { return this.type; }
};

const obj = new ReassignableClass();
console.log(obj.getType());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DynamicClass"),
        "expected DynamicClass in output. output: {output}"
    );
    assert!(
        output.contains("ReassignableClass"),
        "expected ReassignableClass in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression in variable"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_in_array() {
    let source = r#"const classes = [
    class Alpha {
        name = "Alpha";
        greet() { return "I am " + this.name; }
    },
    class Beta {
        name = "Beta";
        greet() { return "I am " + this.name; }
    },
    class Gamma {
        name = "Gamma";
        greet() { return "I am " + this.name; }
    }
];

for (const Cls of classes) {
    const instance = new Cls();
    console.log(instance.greet());
}

const handlers = [
    class { handle(x: number) { return x * 2; } },
    class { handle(x: number) { return x + 10; } },
    class { handle(x: number) { return x - 5; } }
];

const value = 10;
handlers.forEach(Handler => {
    const h = new Handler();
    console.log(h.handle(value));
});"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("classes"),
        "expected classes array in output. output: {output}"
    );
    assert!(
        output.contains("handlers"),
        "expected handlers array in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression in array"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_in_object() {
    let source = r#"const components = {
    Button: class {
        label: string;
        constructor(label: string) { this.label = label; }
        render() { return "<button>" + this.label + "</button>"; }
    },
    Input: class {
        placeholder: string;
        constructor(placeholder: string) { this.placeholder = placeholder; }
        render() { return "<input placeholder=\"" + this.placeholder + "\">"; }
    },
    Label: class {
        text: string;
        constructor(text: string) { this.text = text; }
        render() { return "<label>" + this.text + "</label>"; }
    }
};

const btn = new components.Button("Click me");
console.log(btn.render());

const registry = {
    handlers: {
        click: class { execute() { console.log("click"); } },
        hover: class { execute() { console.log("hover"); } }
    }
};

const clickHandler = new registry.handlers.click();
clickHandler.execute();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("components"),
        "expected components object in output. output: {output}"
    );
    assert!(
        output.contains("registry"),
        "expected registry object in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression in object"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_with_methods() {
    let source = r#"const Calculator = class {
    private result: number = 0;

    constructor(initial: number = 0) {
        this.result = initial;
    }

    add(value: number): this {
        this.result += value;
        return this;
    }

    subtract(value: number): this {
        this.result -= value;
        return this;
    }

    multiply(value: number): this {
        this.result *= value;
        return this;
    }

    divide(value: number): this {
        if (value !== 0) {
            this.result /= value;
        }
        return this;
    }

    getResult(): number {
        return this.result;
    }

    reset(): this {
        this.result = 0;
        return this;
    }
};

const calc = new Calculator(10);
const result = calc.add(5).multiply(2).subtract(10).getResult();
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected Calculator in output. output: {output}"
    );
    assert!(
        output.contains("add"),
        "expected add method in output. output: {output}"
    );
    assert!(
        output.contains("multiply"),
        "expected multiply method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression with methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_comprehensive() {
    let source = r#"// Comprehensive class expression patterns for ES5 transform testing

// Anonymous class expression
const Anonymous = class {
    value = 42;
    getValue() { return this.value; }
};

// Named class expression
const Named = class InnerName {
    static type = "Named";
    describe() { return InnerName.type; }
};

// Class expression with extends
class BaseClass {
    name: string;
    constructor(name: string) { this.name = name; }
}

const Derived = class extends BaseClass {
    extra = "derived";
    constructor(name: string) {
        super(name);
    }
    getInfo() { return this.name + " - " + this.extra; }
};

// Class expression with static members
const WithStatic = class {
    static count = 0;
    id: number;

    constructor() {
        WithStatic.count++;
        this.id = WithStatic.count;
    }

    static getCount() { return WithStatic.count; }
};

// Factory function returning class expression
function createClass<T>(defaultValue: T) {
    return class {
        value: T = defaultValue;
        getValue(): T { return this.value; }
        setValue(v: T): void { this.value = v; }
    };
}

const StringClass = createClass("hello");
const NumberClass = createClass(123);

// Class expressions in array
const classArray = [
    class { type = "A"; },
    class { type = "B"; },
    class { type = "C"; }
];

// Class expressions in object
const classMap = {
    first: class First { id = 1; },
    second: class Second { id = 2; },
    third: class Third { id = 3; }
};

// IIFE with class expression
const IIFEClass = (function() {
    const privateData = "secret";
    return class {
        getPrivate() { return privateData; }
    };
})();

// Class expression with accessors
const WithAccessors = class {
    private _value: number = 0;

    get value(): number { return this._value; }
    set value(v: number) { this._value = v; }

    get doubled(): number { return this._value * 2; }
};

// Usage
const anon = new Anonymous();
console.log(anon.getValue());

const named = new Named();
console.log(named.describe());

const derived = new Derived("test");
console.log(derived.getInfo());

new WithStatic();
new WithStatic();
console.log(WithStatic.getCount());

const strInstance = new StringClass();
console.log(strInstance.getValue());

const numInstance = new NumberClass();
console.log(numInstance.getValue());

for (const Cls of classArray) {
    console.log(new Cls().type);
}

console.log(new classMap.first().id);

const iifeInstance = new IIFEClass();
console.log(iifeInstance.getPrivate());

const accessorInstance = new WithAccessors();
accessorInstance.value = 21;
console.log(accessorInstance.doubled);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Anonymous"),
        "expected Anonymous in output. output: {output}"
    );
    assert!(
        output.contains("Named"),
        "expected Named in output. output: {output}"
    );
    assert!(
        output.contains("Derived"),
        "expected Derived in output. output: {output}"
    );
    assert!(
        output.contains("WithStatic"),
        "expected WithStatic in output. output: {output}"
    );
    assert!(
        output.contains("createClass"),
        "expected createClass function in output. output: {output}"
    );
    assert!(
        output.contains("WithAccessors"),
        "expected WithAccessors in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive class expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Arrow Function Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_arrow_es5_expression_body() {
    let source = r#"const add = (a: number, b: number) => a + b;
const square = (x: number) => x * x;
const identity = <T>(x: T) => x;

const double = (n: number) => n * 2;
const negate = (n: number) => -n;
const toString = (x: any) => String(x);

console.log(add(1, 2));
console.log(square(5));
console.log(double(10));
console.log(negate(5));
console.log(toString(42));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("add"),
        "expected add in output. output: {output}"
    );
    assert!(
        output.contains("square"),
        "expected square in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow expression body"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_block_body() {
    let source = r#"const processData = (data: number[]) => {
    const filtered = data.filter(x => x > 0);
    const doubled = filtered.map(x => x * 2);
    return doubled.reduce((a, b) => a + b, 0);
};

const validateInput = (input: string) => {
    if (!input) {
        throw new Error("Invalid input");
    }
    const trimmed = input.trim();
    if (trimmed.length === 0) {
        return null;
    }
    return trimmed.toUpperCase();
};

const factorial = (n: number): number => {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
};

console.log(processData([1, -2, 3, -4, 5]));
console.log(validateInput("  hello  "));
console.log(factorial(5));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processData"),
        "expected processData in output. output: {output}"
    );
    assert!(
        output.contains("validateInput"),
        "expected validateInput in output. output: {output}"
    );
    assert!(
        output.contains("factorial"),
        "expected factorial in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow block body"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_this_binding() {
    let source = r#"class Counter {
    count = 0;

    increment = () => {
        this.count++;
        return this.count;
    };

    decrement = () => {
        this.count--;
        return this.count;
    };

    addMultiple = (n: number) => {
        for (let i = 0; i < n; i++) {
            this.increment();
        }
        return this.count;
    };
}

class EventHandler {
    name = "Handler";

    handleClick = () => {
        console.log(this.name + " clicked");
    };

    handleHover = () => {
        console.log(this.name + " hovered");
    };
}

const counter = new Counter();
console.log(counter.increment());
console.log(counter.addMultiple(5));

const handler = new EventHandler();
handler.handleClick();
handler.handleHover();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("EventHandler"),
        "expected EventHandler class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow this binding"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_rest_params() {
    let source = r#"const sum = (...numbers: number[]) => numbers.reduce((a, b) => a + b, 0);

const concat = (...strings: string[]) => strings.join("");

const first = <T>(first: T, ...rest: T[]) => first;

const last = <T>(...items: T[]) => items[items.length - 1];

const collect = (prefix: string, ...values: any[]) => {
    return prefix + ": " + values.join(", ");
};

const spread = (...args: number[]) => {
    const [head, ...tail] = args;
    return { head, tail };
};

console.log(sum(1, 2, 3, 4, 5));
console.log(concat("a", "b", "c"));
console.log(first(1, 2, 3));
console.log(last(1, 2, 3, 4));
console.log(collect("Items", 1, 2, 3));
console.log(spread(10, 20, 30, 40));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("sum"),
        "expected sum in output. output: {output}"
    );
    assert!(
        output.contains("concat"),
        "expected concat in output. output: {output}"
    );
    assert!(
        output.contains("collect"),
        "expected collect in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow rest params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_default_params() {
    let source = r#"const greet = (name: string = "World") => "Hello, " + name;

const power = (base: number, exp: number = 2) => Math.pow(base, exp);

const configure = (host: string = "localhost", port: number = 8080) => {
    return { host, port };
};

const format = (value: number, prefix: string = "$", suffix: string = "") => {
    return prefix + value.toFixed(2) + suffix;
};

const createUser = (name: string, age: number = 0, active: boolean = true) => ({
    name,
    age,
    active
});

console.log(greet());
console.log(greet("Alice"));
console.log(power(2));
console.log(power(2, 10));
console.log(configure());
console.log(format(123.456));
console.log(createUser("Bob"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("greet"),
        "expected greet in output. output: {output}"
    );
    assert!(
        output.contains("power"),
        "expected power in output. output: {output}"
    );
    assert!(
        output.contains("configure"),
        "expected configure in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow default params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_destructuring_params() {
    let source = r#"const getFullName = ({ first, last }: { first: string; last: string }) =>
    first + " " + last;

const getCoords = ([x, y]: [number, number]) => ({ x, y });

const processUser = ({ name, age = 0 }: { name: string; age?: number }) => {
    return name + " is " + age + " years old";
};

const extractValues = ({ a, b, ...rest }: { a: number; b: number; [key: string]: number }) => {
    return { sum: a + b, rest };
};

const handleEvent = ({ type, target: { id } }: { type: string; target: { id: string } }) => {
    console.log(type + " on " + id);
};

console.log(getFullName({ first: "John", last: "Doe" }));
console.log(getCoords([10, 20]));
console.log(processUser({ name: "Alice" }));
console.log(extractValues({ a: 1, b: 2, c: 3, d: 4 }));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getFullName"),
        "expected getFullName in output. output: {output}"
    );
    assert!(
        output.contains("processUser"),
        "expected processUser in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow destructuring params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_class_property() {
    let source = r#"class Calculator {
    add = (a: number, b: number) => a + b;
    subtract = (a: number, b: number) => a - b;
    multiply = (a: number, b: number) => a * b;
    divide = (a: number, b: number) => b !== 0 ? a / b : 0;
}

class Formatter {
    prefix: string;
    suffix: string;

    constructor(prefix: string = "", suffix: string = "") {
        this.prefix = prefix;
        this.suffix = suffix;
    }

    format = (value: any) => this.prefix + String(value) + this.suffix;
    formatNumber = (n: number) => this.format(n.toFixed(2));
}

class Logger {
    static log = (message: string) => console.log("[LOG] " + message);
    static warn = (message: string) => console.warn("[WARN] " + message);
    static error = (message: string) => console.error("[ERROR] " + message);
}

const calc = new Calculator();
console.log(calc.add(1, 2));
console.log(calc.multiply(3, 4));

const fmt = new Formatter("$", " USD");
console.log(fmt.formatNumber(123.456));

Logger.log("Test message");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected Calculator in output. output: {output}"
    );
    assert!(
        output.contains("Formatter"),
        "expected Formatter in output. output: {output}"
    );
    assert!(
        output.contains("Logger"),
        "expected Logger in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow class property"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_higher_order() {
    let source = r#"const createAdder = (n: number) => (x: number) => x + n;

const createMultiplier = (factor: number) => (x: number) => x * factor;

const compose = <A, B, C>(f: (b: B) => C, g: (a: A) => B) => (x: A) => f(g(x));

const pipe = <T>(...fns: ((x: T) => T)[]) => (x: T) => fns.reduce((v, f) => f(v), x);

const curry = (fn: (a: number, b: number) => number) => (a: number) => (b: number) => fn(a, b);

const memoize = <T, R>(fn: (arg: T) => R) => {
    const cache = new Map<T, R>();
    return (arg: T) => {
        if (cache.has(arg)) return cache.get(arg)!;
        const result = fn(arg);
        cache.set(arg, result);
        return result;
    };
};

const add5 = createAdder(5);
const double = createMultiplier(2);
const addThenDouble = compose(double, add5);

console.log(add5(10));
console.log(double(7));
console.log(addThenDouble(3));

const increment = (x: number) => x + 1;
const triple = (x: number) => x * 3;
const pipeline = pipe(increment, double, triple);
console.log(pipeline(2));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createAdder"),
        "expected createAdder in output. output: {output}"
    );
    assert!(
        output.contains("compose"),
        "expected compose in output. output: {output}"
    );
    assert!(
        output.contains("memoize"),
        "expected memoize in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for higher order arrows"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_callbacks() {
    let source = r#"const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

const doubled = numbers.map(n => n * 2);
const evens = numbers.filter(n => n % 2 === 0);
const sum = numbers.reduce((acc, n) => acc + n, 0);
const found = numbers.find(n => n > 5);
const allPositive = numbers.every(n => n > 0);
const hasNegative = numbers.some(n => n < 0);

const sorted = [...numbers].sort((a, b) => b - a);

const users = [
    { name: "Alice", age: 30 },
    { name: "Bob", age: 25 },
    { name: "Charlie", age: 35 }
];

const names = users.map(u => u.name);
const adults = users.filter(u => u.age >= 18);
const totalAge = users.reduce((sum, u) => sum + u.age, 0);
const youngest = users.reduce((min, u) => u.age < min.age ? u : min);

setTimeout(() => console.log("delayed"), 0);
Promise.resolve(42).then(n => n * 2).then(n => console.log(n));

console.log(doubled, evens, sum, found);
console.log(names, adults, totalAge, youngest);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("doubled"),
        "expected doubled in output. output: {output}"
    );
    assert!(
        output.contains("users"),
        "expected users in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow callbacks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_comprehensive() {
    let source = r#"// Comprehensive arrow function patterns for ES5 transform testing

// Expression body
const add = (a: number, b: number) => a + b;
const identity = <T>(x: T) => x;

// Block body
const process = (data: number[]) => {
    const result = data.map(x => x * 2);
    return result.filter(x => x > 5);
};

// Default parameters
const greet = (name: string = "World") => "Hello, " + name;
const configure = (host: string = "localhost", port: number = 8080) => ({ host, port });

// Rest parameters
const sum = (...nums: number[]) => nums.reduce((a, b) => a + b, 0);
const collect = (first: string, ...rest: string[]) => [first, ...rest];

// Destructuring parameters
const getPoint = ({ x, y }: { x: number; y: number }) => x + y;
const getFirst = ([a, b]: [number, number]) => a;

// This binding in class
class Timer {
    seconds = 0;

    tick = () => {
        this.seconds++;
        return this.seconds;
    };

    reset = () => {
        this.seconds = 0;
    };
}

// Higher-order functions
const createMultiplier = (factor: number) => (x: number) => x * factor;
const compose = <A, B, C>(f: (b: B) => C, g: (a: A) => B) => (x: A) => f(g(x));

// Callbacks
const numbers = [1, 2, 3, 4, 5];
const doubled = numbers.map(n => n * 2);
const evens = numbers.filter(n => n % 2 === 0);
const total = numbers.reduce((acc, n) => acc + n, 0);

// Async arrows
const fetchData = async (url: string) => {
    const response = await fetch(url);
    return response.json();
};

// Arrow IIFE
const result = ((x: number) => x * x)(5);

// Nested arrows
const outer = (a: number) => (b: number) => (c: number) => a + b + c;

// Arrow returning object
const createUser = (name: string, age: number) => ({ name, age, active: true });

// Generic arrow
const mapArray = <T, U>(arr: T[], fn: (x: T) => U) => arr.map(fn);

// Usage
console.log(add(1, 2));
console.log(process([1, 2, 3, 4, 5, 6]));
console.log(greet());
console.log(sum(1, 2, 3, 4, 5));
console.log(getPoint({ x: 10, y: 20 }));

const timer = new Timer();
console.log(timer.tick());
timer.reset();

const triple = createMultiplier(3);
console.log(triple(4));

console.log(doubled, evens, total);
console.log(result);
console.log(outer(1)(2)(3));
console.log(createUser("Alice", 30));
console.log(mapArray([1, 2, 3], x => x * 2));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("add"),
        "expected add in output. output: {output}"
    );
    assert!(
        output.contains("process"),
        "expected process in output. output: {output}"
    );
    assert!(
        output.contains("Timer"),
        "expected Timer class in output. output: {output}"
    );
    assert!(
        output.contains("createMultiplier"),
        "expected createMultiplier in output. output: {output}"
    );
    assert!(
        output.contains("fetchData"),
        "expected fetchData in output. output: {output}"
    );
    assert!(
        output.contains("mapArray"),
        "expected mapArray in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive arrow functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 Template Literal Transform Source Map Tests
// =============================================================================

#[test]
fn test_source_map_template_literal_es5_basic() {
    let source = r#"const greeting = `Hello, World!`;
const simple = `Just a string`;
const empty = ``;
const withNewline = `Line 1
Line 2`;
console.log(greeting, simple, empty, withNewline);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Hello"),
        "expected Hello in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic template literals"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_expression() {
    let source = r#"const name = "Alice";
const age = 30;
const message = `Hello, ${name}!`;
const info = `${name} is ${age} years old`;
const calc = `Result: ${2 + 3 * 4}`;
const nested = `Outer ${`Inner ${name}`}`;
console.log(message, info, calc, nested);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("name"),
        "expected name in output. output: {output}"
    );
    assert!(
        output.contains("message"),
        "expected message in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for template expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

#[test]
fn test_source_map_class_static_block_es5_init_order() {
    let source = r#"const log: string[] = [];

class InitOrder {
    static a = (log.push("field a"), 1);

    static {
        log.push("block 1");
    }

    static b = (log.push("field b"), 2);

    static {
        log.push("block 2");
    }

    static c = (log.push("field c"), 3);

    static {
        log.push("block 3");
        console.log("Final order:", log.join(", "));
    }
}

console.log(InitOrder.a, InitOrder.b, InitOrder.c);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("InitOrder"),
        "expected InitOrder in output. output: {output}"
    );
    assert!(
        output.contains("log"),
        "expected log in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for init order static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_private_access() {
    let source = r#"class SecretHolder {
    static #secret = "initial";
    static #counter = 0;

    static {
        SecretHolder.#secret = "configured";
        SecretHolder.#counter = 100;
    }

    static getSecret() {
        return SecretHolder.#secret;
    }

    static getCounter() {
        return SecretHolder.#counter;
    }
}

console.log(SecretHolder.getSecret(), SecretHolder.getCounter());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("SecretHolder"),
        "expected SecretHolder in output. output: {output}"
    );
    assert!(
        output.contains("getSecret"),
        "expected getSecret in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private access static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_super_access() {
    let source = r#"class Base {
    static baseValue = 10;

    static getBaseValue() {
        return Base.baseValue;
    }
}

class Derived extends Base {
    static derivedValue: number;

    static {
        Derived.derivedValue = Derived.baseValue * 2;
        console.log("Derived initialized with:", Derived.derivedValue);
    }

    static getCombined() {
        return Derived.baseValue + Derived.derivedValue;
    }
}

console.log(Derived.getCombined());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Base"),
        "expected Base in output. output: {output}"
    );
    assert!(
        output.contains("Derived"),
        "expected Derived in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for super access static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_static_field_init() {
    let source = r#"class Database {
    static connection: any;
    static config: { host: string; port: number };
    static ready = false;

    static {
        Database.config = {
            host: "localhost",
            port: 5432
        };
    }

    static {
        Database.connection = {
            config: Database.config,
            connected: false
        };
    }

    static {
        Database.ready = true;
        console.log("Database ready:", Database.config.host);
    }

    static connect() {
        if (Database.ready) {
            Database.connection.connected = true;
        }
    }
}

Database.connect();
console.log(Database.connection);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Database"),
        "expected Database in output. output: {output}"
    );
    assert!(
        output.contains("connection"),
        "expected connection in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static field init blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_computed_props() {
    let source = r#"const KEY1 = "config1";
const KEY2 = "config2";

class ConfigMap {
    static [KEY1]: string;
    static [KEY2]: number;
    static data: Record<string, any> = {};

    static {
        ConfigMap[KEY1] = "value1";
        ConfigMap[KEY2] = 42;
        ConfigMap.data[KEY1] = ConfigMap[KEY1];
        ConfigMap.data[KEY2] = ConfigMap[KEY2];
    }

    static get(key: string) {
        return ConfigMap.data[key];
    }
}

console.log(ConfigMap.get(KEY1), ConfigMap.get(KEY2));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ConfigMap"),
        "expected ConfigMap in output. output: {output}"
    );
    assert!(
        output.contains("KEY1"),
        "expected KEY1 in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed props static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_async_patterns() {
    let source = r#"class AsyncLoader {
    static data: any[] = [];
    static initialized = false;

    static {
        const init = async () => {
            AsyncLoader.data = [1, 2, 3];
            AsyncLoader.initialized = true;
        };
        init();
    }

    static async load() {
        if (!AsyncLoader.initialized) {
            await new Promise(r => setTimeout(r, 100));
        }
        return AsyncLoader.data;
    }
}

class EventEmitter {
    static handlers: Map<string, Function[]> = new Map();

    static {
        EventEmitter.handlers.set("init", []);
        EventEmitter.handlers.set("load", []);
    }

    static on(event: string, handler: Function) {
        const handlers = EventEmitter.handlers.get(event) || [];
        handlers.push(handler);
        EventEmitter.handlers.set(event, handlers);
    }
}

console.log(AsyncLoader.data, EventEmitter.handlers);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AsyncLoader"),
        "expected AsyncLoader in output. output: {output}"
    );
    assert!(
        output.contains("EventEmitter"),
        "expected EventEmitter in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async patterns static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_error_handling() {
    let source = r#"class SafeInit {
    static value: number;
    static error: Error | null = null;

    static {
        try {
            SafeInit.value = parseInt("42");
            if (isNaN(SafeInit.value)) {
                throw new Error("Invalid number");
            }
        } catch (e) {
            SafeInit.error = e as Error;
            SafeInit.value = 0;
        }
    }

    static getValue() {
        if (SafeInit.error) {
            console.error("Initialization failed:", SafeInit.error.message);
            return null;
        }
        return SafeInit.value;
    }
}

class Validator {
    static rules: Map<string, Function> = new Map();
    static errors: string[] = [];

    static {
        try {
            Validator.rules.set("required", (v: any) => v != null);
            Validator.rules.set("minLength", (v: string) => v.length >= 3);
        } catch (e) {
            Validator.errors.push("Failed to initialize rules");
        }
    }
}

console.log(SafeInit.getValue(), Validator.rules.size);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("SafeInit"),
        "expected SafeInit in output. output: {output}"
    );
    assert!(
        output.contains("Validator"),
        "expected Validator in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for error handling static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_block_es5_comprehensive() {
    let source = r#"// Comprehensive class static block patterns for ES5 transform testing

// Basic static block
class Counter {
    static count = 0;

    static {
        Counter.count = 100;
    }
}

// Multiple blocks with init order
const initLog: string[] = [];

class MultiBlock {
    static a = (initLog.push("a"), 1);

    static {
        initLog.push("block1");
    }

    static b = (initLog.push("b"), 2);

    static {
        initLog.push("block2");
    }
}

// Private field access
class PrivateAccess {
    static #privateValue = 0;

    static {
        PrivateAccess.#privateValue = 42;
    }

    static getValue() {
        return PrivateAccess.#privateValue;
    }
}

// Inheritance
class Parent {
    static parentValue = 10;
}

class Child extends Parent {
    static childValue: number;

    static {
        Child.childValue = Child.parentValue * 2;
    }
}

// Complex initialization
class ComplexInit {
    static config: { host: string; port: number };
    static connections: Map<string, any>;
    static ready = false;

    static {
        ComplexInit.config = {
            host: "localhost",
            port: 3000
        };
    }

    static {
        ComplexInit.connections = new Map();
        ComplexInit.connections.set("default", ComplexInit.config);
    }

    static {
        ComplexInit.ready = true;
        console.log("ComplexInit ready");
    }
}

// Error handling
class SafeLoader {
    static data: any[] = [];
    static error: Error | null = null;

    static {
        try {
            SafeLoader.data = [1, 2, 3];
        } catch (e) {
            SafeLoader.error = e as Error;
        }
    }
}

// Async initialization pattern
class AsyncInit {
    static promise: Promise<void>;

    static {
        AsyncInit.promise = (async () => {
            await Promise.resolve();
            console.log("Async init complete");
        })();
    }
}

// Computed property access
const KEY = "dynamicKey";

class DynamicClass {
    static [KEY]: string;

    static {
        DynamicClass[KEY] = "dynamic value";
    }
}

// Usage
console.log(Counter.count);
console.log(initLog);
console.log(PrivateAccess.getValue());
console.log(Child.childValue);
console.log(ComplexInit.ready);
console.log(SafeLoader.data);
console.log(DynamicClass[KEY]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("MultiBlock"),
        "expected MultiBlock in output. output: {output}"
    );
    assert!(
        output.contains("PrivateAccess"),
        "expected PrivateAccess in output. output: {output}"
    );
    assert!(
        output.contains("Parent"),
        "expected Parent in output. output: {output}"
    );
    assert!(
        output.contains("Child"),
        "expected Child in output. output: {output}"
    );
    assert!(
        output.contains("ComplexInit"),
        "expected ComplexInit in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 Decorator Composition Transform Source Map Tests
// =============================================================================

#[test]
fn test_source_map_decorator_composition_es5_chained() {
    let source = r#"function log(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Calling:", key);
        return original.apply(this, args);
    };
    return descriptor;
}

function measure(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        const start = Date.now();
        const result = original.apply(this, args);
        console.log("Duration:", Date.now() - start);
        return result;
    };
    return descriptor;
}

function validate(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        if (args.some(a => a == null)) throw new Error("Invalid args");
        return original.apply(this, args);
    };
    return descriptor;
}

class Service {
    @log
    @measure
    @validate
    process(data: string) {
        return data.toUpperCase();
    }
}

const service = new Service();
console.log(service.process("test"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Service"),
        "expected Service in output. output: {output}"
    );
    assert!(
        output.contains("process"),
        "expected process in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_factory() {
    let source = r#"function log(prefix: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.log(prefix, key, args);
            return original.apply(this, args);
        };
        return descriptor;
    };
}

function retry(times: number) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = async function(...args: any[]) {
            for (let i = 0; i < times; i++) {
                try {
                    return await original.apply(this, args);
                } catch (e) {
                    if (i === times - 1) throw e;
                }
            }
        };
        return descriptor;
    };
}

function cache(ttl: number) {
    const store = new Map<string, { value: any; expires: number }>();
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            const cacheKey = JSON.stringify(args);
            const cached = store.get(cacheKey);
            if (cached && cached.expires > Date.now()) return cached.value;
            const result = original.apply(this, args);
            store.set(cacheKey, { value: result, expires: Date.now() + ttl });
            return result;
        };
        return descriptor;
    };
}

class Api {
    @log("[API]")
    @retry(3)
    @cache(5000)
    async fetchData(id: number) {
        return { id, data: "result" };
    }
}

const api = new Api();
api.fetchData(1).then(console.log);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Api"),
        "expected Api in output. output: {output}"
    );
    assert!(
        output.contains("fetchData"),
        "expected fetchData in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for factory decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_metadata() {
    let source = r#"const metadata = new Map<any, Map<string, any>>();

function setMetadata(key: string, value: any) {
    return function(target: any, propertyKey?: string) {
        let targetMeta = metadata.get(target) || new Map();
        targetMeta.set(key, value);
        metadata.set(target, targetMeta);
    };
}

function getMetadata(key: string, target: any): any {
    const targetMeta = metadata.get(target);
    return targetMeta?.get(key);
}

@setMetadata("version", "1.0.0")
@setMetadata("author", "Team")
class Component {
    @setMetadata("required", true)
    name: string = "";

    @setMetadata("type", "handler")
    @setMetadata("async", true)
    async handleEvent(event: any) {
        console.log(event);
    }
}

const comp = new Component();
console.log(getMetadata("version", Component));
console.log(getMetadata("author", Component));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Component"),
        "expected Component in output. output: {output}"
    );
    assert!(
        output.contains("metadata"),
        "expected metadata in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for metadata decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_method_params() {
    let source = r#"const paramMetadata = new Map<any, Map<string, any[]>>();

function param(name: string) {
    return function(target: any, methodKey: string, paramIndex: number) {
        let methodMeta = paramMetadata.get(target) || new Map();
        let params = methodMeta.get(methodKey) || [];
        params[paramIndex] = name;
        methodMeta.set(methodKey, params);
        paramMetadata.set(target, methodMeta);
    };
}

function inject(token: string) {
    return function(target: any, methodKey: string, paramIndex: number) {
        console.log("Inject", token, "at index", paramIndex);
    };
}

function validate(schema: any) {
    return function(target: any, methodKey: string, paramIndex: number) {
        console.log("Validate param", paramIndex, "with schema");
    };
}

class UserService {
    createUser(
        @param("name") @validate({ type: "string" }) name: string,
        @param("email") @validate({ type: "email" }) email: string,
        @param("age") @validate({ type: "number" }) age: number
    ) {
        return { name, email, age };
    }

    findUser(@inject("db") db: any, @param("id") id: number) {
        return db.find(id);
    }
}

const userService = new UserService();
console.log(userService.createUser("John", "john@example.com", 30));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        output.contains("createUser"),
        "expected createUser in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method param decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_accessor() {
    let source = r#"function observable(target: any, key: string, descriptor: PropertyDescriptor) {
    const getter = descriptor.get;
    const setter = descriptor.set;

    if (setter) {
        descriptor.set = function(value: any) {
            console.log("Setting", key, "to", value);
            setter.call(this, value);
        };
    }

    if (getter) {
        descriptor.get = function() {
            const value = getter.call(this);
            console.log("Getting", key, "=", value);
            return value;
        };
    }

    return descriptor;
}

function lazy(target: any, key: string, descriptor: PropertyDescriptor) {
    const getter = descriptor.get;
    const cacheKey = Symbol(key);

    descriptor.get = function() {
        if (!(this as any)[cacheKey]) {
            (this as any)[cacheKey] = getter?.call(this);
        }
        return (this as any)[cacheKey];
    };

    return descriptor;
}

class Config {
    private _value = 0;

    @observable
    @lazy
    get computedValue() {
        return this._value * 2;
    }

    @observable
    set value(v: number) {
        this._value = v;
    }

    get value() {
        return this._value;
    }
}

const config = new Config();
config.value = 5;
console.log(config.computedValue);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("observable"),
        "expected observable in output. output: {output}"
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
fn test_source_map_decorator_composition_es5_multiple_targets() {
    let source = r#"function sealed(constructor: Function) {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

function enumerable(value: boolean) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        descriptor.enumerable = value;
        return descriptor;
    };
}

function readonly(target: any, key: string) {
    Object.defineProperty(target, key, { writable: false });
}

@sealed
class Person {
    @readonly
    id: number = 1;

    name: string = "";

    @enumerable(false)
    get fullInfo() {
        return this.id + ": " + this.name;
    }

    @enumerable(true)
    greet() {
        return "Hello, " + this.name;
    }
}

const person = new Person();
person.name = "Alice";
console.log(person.greet());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected Person in output. output: {output}"
    );
    assert!(
        output.contains("sealed"),
        "expected sealed in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple target decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_conditional() {
    let source = r#"const isProduction = false;
const enableLogging = true;

function conditionalLog(target: any, key: string, descriptor: PropertyDescriptor) {
    if (!enableLogging) return descriptor;

    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Calling:", key);
        return original.apply(this, args);
    };
    return descriptor;
}

function devOnly(target: any, key: string, descriptor: PropertyDescriptor) {
    if (isProduction) {
        descriptor.value = function() {
            throw new Error("Not available in production");
        };
    }
    return descriptor;
}

function deprecated(message: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.warn("Deprecated:", message);
            return original.apply(this, args);
        };
        return descriptor;
    };
}

class FeatureService {
    @conditionalLog
    @devOnly
    debugInfo() {
        return { env: "development", debug: true };
    }

    @conditionalLog
    @deprecated("Use newMethod instead")
    oldMethod() {
        return "old";
    }

    @conditionalLog
    newMethod() {
        return "new";
    }
}

const service = new FeatureService();
console.log(service.debugInfo());
console.log(service.newMethod());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("FeatureService"),
        "expected FeatureService in output. output: {output}"
    );
    assert!(
        output.contains("deprecated"),
        "expected deprecated in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_generic() {
    let source = r#"function typed<T>(type: new () => T) {
    return function(target: any, key: string) {
        console.log("Property", key, "is typed as", type.name);
    };
}

function transform<T, U>(fn: (value: T) => U) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(input: T): U {
            const result = original.call(this, input);
            return fn(result);
        };
        return descriptor;
    };
}

function collect<T>() {
    const items: T[] = [];
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]): T {
            const result = original.apply(this, args);
            items.push(result);
            return result;
        };
        return descriptor;
    };
}

class DataProcessor {
    @typed(String)
    name: string = "";

    @transform((x: number) => x.toString())
    @collect<string>()
    process(value: number) {
        return value * 2;
    }
}

const processor = new DataProcessor();
console.log(processor.process(5));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected DataProcessor in output. output: {output}"
    );
    assert!(
        output.contains("transform"),
        "expected transform in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_inheritance() {
    let source = r#"function logMethod(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Method:", key, "called on", this.constructor.name);
        return original.apply(this, args);
    };
    return descriptor;
}

function track(target: any, key: string) {
    console.log("Tracking property:", key);
}

abstract class BaseEntity {
    @track
    id: number = 0;

    @logMethod
    save() {
        console.log("Saving entity", this.id);
    }
}

class User extends BaseEntity {
    @track
    name: string = "";

    @logMethod
    save() {
        console.log("Saving user", this.name);
        super.save();
    }

    @logMethod
    validate() {
        return this.name.length > 0;
    }
}

class Admin extends User {
    @track
    role: string = "admin";

    @logMethod
    save() {
        console.log("Saving admin with role", this.role);
        super.save();
    }
}

const admin = new Admin();
admin.name = "John";
admin.save();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("BaseEntity"),
        "expected BaseEntity in output. output: {output}"
    );
    assert!(
        output.contains("User"),
        "expected User in output. output: {output}"
    );
    assert!(
        output.contains("Admin"),
        "expected Admin in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for inheritance decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_comprehensive() {
    let source = r#"// Comprehensive decorator composition patterns for ES5 transform testing

// Metadata storage
const metadata = new Map<any, Map<string, any>>();

// Basic decorators
function log(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Call:", key);
        return original.apply(this, args);
    };
    return descriptor;
}

function sealed(constructor: Function) {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

// Factory decorators
function prefix(p: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            return p + original.apply(this, args);
        };
        return descriptor;
    };
}

function retry(times: number) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = async function(...args: any[]) {
            for (let i = 0; i < times; i++) {
                try { return await original.apply(this, args); }
                catch (e) { if (i === times - 1) throw e; }
            }
        };
        return descriptor;
    };
}

// Metadata decorators
function setMeta(key: string, value: any) {
    return function(target: any) {
        let targetMeta = metadata.get(target) || new Map();
        targetMeta.set(key, value);
        metadata.set(target, targetMeta);
    };
}

// Property decorator
function track(target: any, key: string) {
    console.log("Tracking:", key);
}

// Parameter decorator
function param(name: string) {
    return function(target: any, methodKey: string, index: number) {
        console.log("Param", name, "at", index);
    };
}

// Accessor decorator
function observable(target: any, key: string, descriptor: PropertyDescriptor) {
    const getter = descriptor.get;
    descriptor.get = function() {
        console.log("Access:", key);
        return getter?.call(this);
    };
    return descriptor;
}

// Comprehensive class with all decorator types
@sealed
@setMeta("version", "1.0")
@setMeta("author", "Team")
class ComplexService {
    @track
    id: number = 0;

    @track
    name: string = "";

    private _status = "idle";

    @observable
    get status() {
        return this._status;
    }

    set status(v: string) {
        this._status = v;
    }

    @log
    @prefix("[INFO] ")
    getMessage() {
        return "Hello from " + this.name;
    }

    @log
    @retry(3)
    async fetchData(@param("url") url: string) {
        return { url, data: "result" };
    }

    @log
    process(
        @param("input") input: string,
        @param("options") options: any
    ) {
        return input.toUpperCase();
    }
}

// Inheritance with decorators
abstract class BaseService {
    @track
    baseId: number = 0;

    @log
    init() {
        console.log("Init base");
    }
}

class ChildService extends BaseService {
    @track
    childId: number = 0;

    @log
    init() {
        super.init();
        console.log("Init child");
    }
}

// Usage
const service = new ComplexService();
service.name = "Test";
console.log(service.getMessage());
console.log(service.status);

const child = new ChildService();
child.init();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ComplexService"),
        "expected ComplexService in output. output: {output}"
    );
    assert!(
        output.contains("BaseService"),
        "expected BaseService in output. output: {output}"
    );
    assert!(
        output.contains("ChildService"),
        "expected ChildService in output. output: {output}"
    );
    assert!(
        output.contains("log"),
        "expected log decorator in output. output: {output}"
    );
    assert!(
        output.contains("sealed"),
        "expected sealed decorator in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive decorator composition"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 Private Method Transform Source Map Tests
// =============================================================================

#[test]
fn test_source_map_private_method_es5_instance_basic() {
    let source = r#"class Calculator {
    #add(a: number, b: number): number {
        return a + b;
    }

    #subtract(a: number, b: number): number {
        return a - b;
    }

    #multiply(a: number, b: number): number {
        return a * b;
    }

    calculate(op: string, a: number, b: number): number {
        switch (op) {
            case "+": return this.#add(a, b);
            case "-": return this.#subtract(a, b);
            case "*": return this.#multiply(a, b);
            default: return 0;
        }
    }
}

const calc = new Calculator();
console.log(calc.calculate("+", 5, 3));
console.log(calc.calculate("-", 10, 4));
console.log(calc.calculate("*", 6, 7));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected Calculator in output. output: {output}"
    );
    assert!(
        output.contains("calculate"),
        "expected calculate in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_static() {
    let source = r#"class IdGenerator {
    static #counter = 0;

    static #generateId(): string {
        IdGenerator.#counter++;
        return "id_" + IdGenerator.#counter;
    }

    static #formatId(id: string): string {
        return "[" + id + "]";
    }

    static #validateId(id: string): boolean {
        return id.startsWith("id_");
    }

    static createId(): string {
        const id = IdGenerator.#generateId();
        if (IdGenerator.#validateId(id)) {
            return IdGenerator.#formatId(id);
        }
        return "";
    }

    static getCount(): number {
        return IdGenerator.#counter;
    }
}

console.log(IdGenerator.createId());
console.log(IdGenerator.createId());
console.log(IdGenerator.getCount());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected IdGenerator in output. output: {output}"
    );
    assert!(
        output.contains("createId"),
        "expected createId in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static private methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

/// Test async generator with await expressions and ES5 target
#[test]
fn test_source_map_async_generator_es5_await_expressions() {
    let source = r#"async function fetchData(id: number): Promise<string> {
    return `data-${id}`;
}

async function* fetchSequence(ids: number[]) {
    for (const id of ids) {
        const data = await fetchData(id);
        yield data;
    }
}

async function process() {
    const ids = [1, 2, 3];
    for await (const data of fetchSequence(ids)) {
        console.log(data);
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData"),
        "expected fetchData in output. output: {output}"
    );
    assert!(
        output.contains("fetchSequence"),
        "expected fetchSequence in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for await expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator with for-await-of loops and ES5 target
#[test]
fn test_source_map_async_generator_es5_for_await_of() {
    let source = r#"async function* createStream(): AsyncGenerator<number> {
    yield 1;
    yield 2;
    yield 3;
}

async function processStream() {
    const stream = createStream();
    let total = 0;

    for await (const value of stream) {
        total += value;
    }

    return total;
}

async function nestedForAwait() {
    const streams = [createStream(), createStream()];
    for (const stream of streams) {
        for await (const value of stream) {
            console.log(value);
        }
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createStream"),
        "expected createStream in output. output: {output}"
    );
    assert!(
        output.contains("processStream"),
        "expected processStream in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-await-of"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator with error handling and ES5 target
#[test]
fn test_source_map_async_generator_es5_error_handling() {
    let source = r#"async function* riskyGenerator() {
    try {
        yield 1;
        throw new Error("oops");
        yield 2;
    } catch (e) {
        yield "caught";
    } finally {
        yield "cleanup";
    }
}

async function handleErrors() {
    try {
        for await (const value of riskyGenerator()) {
            console.log(value);
        }
    } catch (e) {
        console.error(e);
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("riskyGenerator"),
        "expected riskyGenerator in output. output: {output}"
    );
    assert!(
        output.contains("handleErrors"),
        "expected handleErrors in output. output: {output}"
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

/// Test async generator as class methods with ES5 target
#[test]
fn test_source_map_async_generator_es5_class_methods() {
    let source = r#"class DataSource {
    private items: string[] = ["a", "b", "c"];

    async *iterate() {
        for (const item of this.items) {
            yield item;
        }
    }

    async *transform(fn: (s: string) => string) {
        for (const item of this.items) {
            yield fn(item);
        }
    }
}

class Pipeline {
    async *chain(sources: DataSource[]) {
        for (const source of sources) {
            yield* source.iterate();
        }
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataSource"),
        "expected DataSource in output. output: {output}"
    );
    assert!(
        output.contains("Pipeline"),
        "expected Pipeline in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator with multiple yields and awaits interleaved with ES5 target
#[test]
fn test_source_map_async_generator_es5_interleaved() {
    let source = r#"async function delay(ms: number): Promise<void> {
    return new Promise(r => setTimeout(r, ms));
}

async function* interleaved() {
    yield "starting";
    await delay(100);
    yield "step 1";
    await delay(100);
    yield "step 2";
    await delay(100);
    yield "step 3";
    await delay(100);
    yield "done";
}

async function run() {
    const results: string[] = [];
    for await (const step of interleaved()) {
        results.push(step);
    }
    return results;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("delay"),
        "expected delay in output. output: {output}"
    );
    assert!(
        output.contains("interleaved"),
        "expected interleaved in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interleaved"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async generator with return values and ES5 target
#[test]
fn test_source_map_async_generator_es5_return_values() {
    let source = r#"async function* withReturn(): AsyncGenerator<number, string, void> {
    yield 1;
    yield 2;
    return "completed";
}

async function* earlyReturn(condition: boolean) {
    yield "start";
    if (condition) {
        return "early exit";
    }
    yield "middle";
    yield "end";
    return "normal exit";
}

async function collectResults() {
    const gen = withReturn();
    let result: IteratorResult<number, string>;
    while (!(result = await gen.next()).done) {
        console.log(result.value);
    }
    console.log("Return value:", result.value);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("withReturn"),
        "expected withReturn in output. output: {output}"
    );
    assert!(
        output.contains("earlyReturn"),
        "expected earlyReturn in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for return values"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nested async generators with ES5 target
#[test]
fn test_source_map_async_generator_es5_nested() {
    let source = r#"async function* outerAsync() {
    async function* innerAsync() {
        yield "inner 1";
        yield "inner 2";
    }

    yield "outer start";
    yield* innerAsync();
    yield "outer end";
}

async function* recursiveGen(depth: number): AsyncGenerator<string> {
    yield `depth ${depth}`;
    if (depth > 0) {
        yield* recursiveGen(depth - 1);
    }
}

async function consume() {
    for await (const value of outerAsync()) {
        console.log(value);
    }
    for await (const value of recursiveGen(3)) {
        console.log(value);
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("outerAsync"),
        "expected outerAsync in output. output: {output}"
    );
    assert!(
        output.contains("recursiveGen"),
        "expected recursiveGen in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested generators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test for async generator patterns with ES5 target
#[test]
fn test_source_map_async_generator_es5_comprehensive() {
    let source = r#"// Utility types and interfaces
interface AsyncIterable<T> {
    [Symbol.asyncIterator](): AsyncIterator<T>;
}

// Async event emitter
class AsyncEventEmitter {
    private events: Map<string, Function[]> = new Map();

    on(event: string, handler: Function) {
        if (!this.events.has(event)) {
            this.events.set(event, []);
        }
        this.events.get(event)!.push(handler);
    }

    async *subscribe(event: string): AsyncGenerator<any> {
        const queue: any[] = [];
        let resolve: ((value: any) => void) | null = null;

        this.on(event, (data: any) => {
            if (resolve) {
                resolve(data);
                resolve = null;
            } else {
                queue.push(data);
            }
        });

        while (true) {
            if (queue.length > 0) {
                yield queue.shift();
            } else {
                yield await new Promise(r => { resolve = r; });
            }
        }
    }
}

// Data processing pipeline
class DataPipeline<T> {
    constructor(private source: AsyncGenerator<T>) {}

    async *map<U>(fn: (item: T) => U | Promise<U>): AsyncGenerator<U> {
        for await (const item of this.source) {
            yield await fn(item);
        }
    }

    async *filter(predicate: (item: T) => boolean | Promise<boolean>): AsyncGenerator<T> {
        for await (const item of this.source) {
            if (await predicate(item)) {
                yield item;
            }
        }
    }

    async *take(count: number): AsyncGenerator<T> {
        let taken = 0;
        for await (const item of this.source) {
            if (taken >= count) break;
            yield item;
            taken++;
        }
    }

    async *batch(size: number): AsyncGenerator<T[]> {
        let batch: T[] = [];
        for await (const item of this.source) {
            batch.push(item);
            if (batch.length >= size) {
                yield batch;
                batch = [];
            }
        }
        if (batch.length > 0) {
            yield batch;
        }
    }
}

// Async iterator utilities
async function* merge<T>(...generators: AsyncGenerator<T>[]): AsyncGenerator<T> {
    const pending = generators.map(async (gen, i) => {
        const result = await gen.next();
        return { index: i, result };
    });

    while (pending.length > 0) {
        const { index, result } = await Promise.race(pending);
        if (!result.done) {
            yield result.value;
            pending[index] = (async () => {
                const res = await generators[index].next();
                return { index, result: res };
            })();
        }
    }
}

async function* range(start: number, end: number): AsyncGenerator<number> {
    for (let i = start; i < end; i++) {
        yield i;
    }
}

async function* fromPromises<T>(promises: Promise<T>[]): AsyncGenerator<T> {
    for (const promise of promises) {
        yield await promise;
    }
}

// Usage example
async function main() {
    const numbers = range(0, 100);
    const pipeline = new DataPipeline(numbers);

    const processed = pipeline
        .filter(n => n % 2 === 0)
        .map(n => n * 2)
        .take(10);

    for await (const num of processed) {
        console.log(num);
    }

    const emitter = new AsyncEventEmitter();
    const subscription = emitter.subscribe("data");

    setTimeout(() => {
        for (let i = 0; i < 5; i++) {
            emitter.on("data", () => i);
        }
    }, 100);
}

main().catch(console.error);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AsyncEventEmitter"),
        "expected AsyncEventEmitter in output. output: {output}"
    );
    assert!(
        output.contains("DataPipeline"),
        "expected DataPipeline in output. output: {output}"
    );
    assert!(
        output.contains("merge"),
        "expected merge in output. output: {output}"
    );
    assert!(
        output.contains("range"),
        "expected range in output. output: {output}"
    );
    assert!(
        output.contains("fromPromises"),
        "expected fromPromises in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive async generators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - OPTIONAL CHAINING PATTERNS
// =============================================================================
// Tests for optional chaining patterns with ES5 target to verify source maps
// work correctly with optional chaining transforms.

/// Test basic optional chaining property access with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_property_access() {
    let source = r#"interface User {
    name?: string;
    address?: {
        city?: string;
        zip?: string;
    };
}

function getUserCity(user: User | null) {
    return user?.address?.city;
}

const user: User = { name: "Alice" };
const city = user?.address?.city;
const name = user?.name;
console.log(city, name);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getUserCity"),
        "expected getUserCity in output. output: {output}"
    );
    assert!(
        output.contains("user"),
        "expected user in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining method call with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_method_call() {
    let source = r#"interface Service {
    getData?(): string;
    process?(data: string): void;
}

function callService(service: Service | undefined) {
    const data = service?.getData?.();
    service?.process?.(data ?? "default");
    return data;
}

class Api {
    client?: {
        fetch?(url: string): Promise<any>;
    };

    async request(url: string) {
        const result = await this.client?.fetch?.(url);
        return result;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("callService"),
        "expected callService in output. output: {output}"
    );
    assert!(
        output.contains("Api"),
        "expected Api in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method call"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining element access with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_element_access() {
    let source = r#"interface Data {
    items?: string[];
    matrix?: number[][];
    records?: Record<string, any>;
}

function getItem(data: Data | null, index: number) {
    return data?.items?.[index];
}

function getMatrixCell(data: Data, row: number, col: number) {
    return data?.matrix?.[row]?.[col];
}

function getRecord(data: Data, key: string) {
    return data?.records?.[key];
}

const data: Data = { items: ["a", "b", "c"] };
const first = data?.items?.[0];
const dynamic = data?.records?.["dynamic-key"];"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getItem"),
        "expected getItem in output. output: {output}"
    );
    assert!(
        output.contains("getMatrixCell"),
        "expected getMatrixCell in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for element access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nested optional chaining with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_nested() {
    let source = r#"interface DeepNested {
    level1?: {
        level2?: {
            level3?: {
                level4?: {
                    value?: string;
                };
            };
        };
    };
}

function getDeepValue(obj: DeepNested | null): string | undefined {
    return obj?.level1?.level2?.level3?.level4?.value;
}

const nested: DeepNested = {};
const deep = nested?.level1?.level2?.level3?.level4?.value;
const partial = nested?.level1?.level2;
console.log(deep, partial);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getDeepValue"),
        "expected getDeepValue in output. output: {output}"
    );
    assert!(
        output.contains("nested"),
        "expected nested in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested chaining"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with nullish coalescing with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_with_nullish() {
    let source = r#"interface Config {
    settings?: {
        theme?: string;
        timeout?: number;
    };
}

function getTheme(config: Config | null): string {
    return config?.settings?.theme ?? "default";
}

function getTimeout(config: Config): number {
    return config?.settings?.timeout ?? 5000;
}

const config: Config = {};
const theme = config?.settings?.theme ?? "light";
const timeout = config?.settings?.timeout ?? 3000;
const nested = config?.settings?.theme ?? config?.settings?.timeout ?? "fallback";
console.log(theme, timeout, nested);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getTheme"),
        "expected getTheme in output. output: {output}"
    );
    assert!(
        output.contains("getTimeout"),
        "expected getTimeout in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nullish coalescing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining in function context with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_function_context() {
    let source = r#"interface Handler {
    callback?: (data: string) => void;
    transform?: (input: number) => number;
}

function invokeHandler(handler: Handler | undefined, data: string) {
    handler?.callback?.(data);
}

function transformValue(handler: Handler, value: number): number | undefined {
    return handler?.transform?.(value);
}

const handlers: Handler[] = [];
const result = handlers[0]?.callback?.("test");
const mapped = handlers.map(h => h?.transform?.(42));

function chainedCalls(handler: Handler | null) {
    const fn = handler?.transform;
    return fn?.(100);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("invokeHandler"),
        "expected invokeHandler in output. output: {output}"
    );
    assert!(
        output.contains("transformValue"),
        "expected transformValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function context"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with chained methods with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_chained_methods() {
    let source = r#"interface Builder {
    setName?(name: string): Builder;
    setValue?(value: number): Builder;
    build?(): object;
}

function buildObject(builder: Builder | null) {
    return builder?.setName?.("test")?.setValue?.(42)?.build?.();
}

class FluentApi {
    private data: any;

    with?(key: string): FluentApi | undefined {
        return this;
    }

    get?(): any {
        return this.data;
    }
}

const api = new FluentApi();
const result = api?.with?.("key")?.get?.();
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("buildObject"),
        "expected buildObject in output. output: {output}"
    );
    assert!(
        output.contains("FluentApi"),
        "expected FluentApi in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with delete operator with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_delete() {
    let source = r#"interface Obj {
    prop?: {
        nested?: string;
    };
    items?: string[];
}

function deleteProp(obj: Obj | null) {
    delete obj?.prop?.nested;
}

function deleteElement(obj: Obj | undefined, index: number) {
    delete obj?.items?.[index];
}

const obj: Obj = { prop: { nested: "value" } };
delete obj?.prop?.nested;
delete obj?.items?.[0];
console.log(obj);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("deleteProp"),
        "expected deleteProp in output. output: {output}"
    );
    assert!(
        output.contains("deleteElement"),
        "expected deleteElement in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for delete operator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with call expression with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_call_expression() {
    let source = r#"type Callback = ((value: number) => void) | undefined;

function invokeCallback(cb: Callback, value: number) {
    cb?.(value);
}

const callbacks: Callback[] = [undefined, (v) => console.log(v)];
callbacks[0]?.(1);
callbacks[1]?.(2);

interface EventEmitter {
    on?: (event: string, handler: Function) => void;
    emit?: (event: string, ...args: any[]) => void;
}

function setupEmitter(emitter: EventEmitter | null) {
    emitter?.on?.("data", console.log);
    emitter?.emit?.("ready");
}

const maybeFunc: (() => number) | null = null;
const result = maybeFunc?.();
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("invokeCallback"),
        "expected invokeCallback in output. output: {output}"
    );
    assert!(
        output.contains("setupEmitter"),
        "expected setupEmitter in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for call expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test for optional chaining patterns with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_comprehensive() {
    let source = r#"// Complex optional chaining scenarios
interface User {
    id: number;
    name?: string;
    profile?: {
        avatar?: string;
        settings?: {
            theme?: string;
            notifications?: boolean;
        };
    };
    friends?: User[];
    getFriendById?(id: number): User | undefined;
}

interface AppState {
    currentUser?: User;
    users?: Map<number, User>;
    cache?: {
        get?(key: string): any;
        set?(key: string, value: any): void;
    };
}

class UserService {
    private state: AppState;

    constructor(state: AppState) {
        this.state = state;
    }

    getCurrentUserName(): string {
        return this.state?.currentUser?.name ?? "Anonymous";
    }

    getUserAvatar(): string | undefined {
        return this.state?.currentUser?.profile?.avatar;
    }

    getUserTheme(): string {
        return this.state?.currentUser?.profile?.settings?.theme ?? "light";
    }

    getNotificationsEnabled(): boolean {
        return this.state?.currentUser?.profile?.settings?.notifications ?? true;
    }

    getFriendName(index: number): string | undefined {
        return this.state?.currentUser?.friends?.[index]?.name;
    }

    findFriend(userId: number, friendId: number): User | undefined {
        const user = this.state?.users?.get(userId);
        return user?.getFriendById?.(friendId);
    }

    getCachedValue(key: string): any {
        return this.state?.cache?.get?.(key);
    }

    setCachedValue(key: string, value: any): void {
        this.state?.cache?.set?.(key, value);
    }
}

// Utility functions with optional chaining
function safeAccess<T, K extends keyof T>(obj: T | null | undefined, key: K): T[K] | undefined {
    return obj?.[key];
}

function safeCall<T, R>(fn: ((arg: T) => R) | undefined, arg: T): R | undefined {
    return fn?.(arg);
}

function deepGet(obj: any, ...keys: string[]): any {
    let current = obj;
    for (const key of keys) {
        current = current?.[key];
        if (current === undefined) break;
    }
    return current;
}

// Usage examples
const appState: AppState = {
    currentUser: {
        id: 1,
        name: "Alice",
        profile: {
            avatar: "avatar.png",
            settings: {
                theme: "dark"
            }
        },
        friends: [
            { id: 2, name: "Bob" },
            { id: 3, name: "Charlie" }
        ]
    }
};

const service = new UserService(appState);
console.log(service.getCurrentUserName());
console.log(service.getUserAvatar());
console.log(service.getUserTheme());
console.log(service.getNotificationsEnabled());
console.log(service.getFriendName(0));

// Edge cases
const nullUser: User | null = null;
const undefinedFriends = nullUser?.friends?.[0]?.name;
const chainedMethods = nullUser?.getFriendById?.(1)?.getFriendById?.(2);
const mixedAccess = appState?.currentUser?.friends?.[0]?.profile?.settings?.theme ?? "default";"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        output.contains("safeAccess"),
        "expected safeAccess in output. output: {output}"
    );
    assert!(
        output.contains("safeCall"),
        "expected safeCall in output. output: {output}"
    );
    assert!(
        output.contains("deepGet"),
        "expected deepGet in output. output: {output}"
    );
    assert!(
        output.contains("appState"),
        "expected appState in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive optional chaining"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - NULLISH COALESCING PATTERNS
// =============================================================================
// Tests for nullish coalescing patterns with ES5 target to verify source maps
// work correctly with nullish coalescing transforms.

/// Test basic nullish coalescing with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_basic() {
    let source = r#"const value1: string | null = null;
const value2: string | undefined = undefined;
const value3: string | null | undefined = "hello";

const result1 = value1 ?? "default1";
const result2 = value2 ?? "default2";
const result3 = value3 ?? "default3";

console.log(result1, result2, result3);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("result1"),
        "expected result1 in output. output: {output}"
    );
    assert!(
        output.contains("result2"),
        "expected result2 in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic nullish coalescing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with null values with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_with_null() {
    let source = r#"function getValueOrDefault(input: string | null): string {
    return input ?? "fallback";
}

const nullValue: string | null = null;
const nonNullValue: string | null = "actual";

const result1 = nullValue ?? "was null";
const result2 = nonNullValue ?? "was null";
const result3 = getValueOrDefault(null);
const result4 = getValueOrDefault("test");

console.log(result1, result2, result3, result4);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getValueOrDefault"),
        "expected getValueOrDefault in output. output: {output}"
    );
    assert!(
        output.contains("nullValue"),
        "expected nullValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for null values"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with undefined values with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_with_undefined() {
    let source = r#"function getOrUndefined(): string | undefined {
    return undefined;
}

function getOrValue(): string | undefined {
    return "value";
}

const undefinedValue: string | undefined = undefined;
const definedValue: string | undefined = "defined";

const result1 = undefinedValue ?? "was undefined";
const result2 = definedValue ?? "was undefined";
const result3 = getOrUndefined() ?? "fallback";
const result4 = getOrValue() ?? "fallback";

console.log(result1, result2, result3, result4);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getOrUndefined"),
        "expected getOrUndefined in output. output: {output}"
    );
    assert!(
        output.contains("getOrValue"),
        "expected getOrValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for undefined values"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test chained nullish coalescing with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_chained() {
    let source = r#"const first: string | null = null;
const second: string | undefined = undefined;
const third: string | null = null;
const fourth: string = "found";

const result1 = first ?? second ?? third ?? fourth;
const result2 = first ?? second ?? "early default";
const result3 = first ?? "immediate default" ?? second;

function chainedDefaults(
    a: string | null,
    b: string | undefined,
    c: string | null
): string {
    return a ?? b ?? c ?? "final fallback";
}

const chainResult = chainedDefaults(null, undefined, "c-value");
console.log(result1, result2, result3, chainResult);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("first"),
        "expected first in output. output: {output}"
    );
    assert!(
        output.contains("chainedDefaults"),
        "expected chainedDefaults in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained nullish coalescing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with function calls with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_function_call() {
    let source = r#"function maybeGetValue(): string | null {
    return Math.random() > 0.5 ? "value" : null;
}

function getDefault(): string {
    return "default from function";
}

const result1 = maybeGetValue() ?? "inline default";
const result2 = maybeGetValue() ?? getDefault();

class DataProvider {
    getValue(): string | undefined {
        return undefined;
    }

    getDefault(): string {
        return "class default";
    }

    getResult(): string {
        return this.getValue() ?? this.getDefault();
    }
}

const provider = new DataProvider();
const classResult = provider.getResult();
console.log(result1, result2, classResult);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("maybeGetValue"),
        "expected maybeGetValue in output. output: {output}"
    );
    assert!(
        output.contains("DataProvider"),
        "expected DataProvider in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function call"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing in assignments with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_assignment() {
    let source = r#"let value: string | null = null;
let result: string;

result = value ?? "assigned default";

function assignWithDefault(input: number | undefined): number {
    let output: number;
    output = input ?? 0;
    return output;
}

class Container {
    private data: string | null = null;

    setData(value: string | null): void {
        this.data = value ?? "container default";
    }

    getData(): string {
        return this.data ?? "no data";
    }
}

const container = new Container();
container.setData(null);
const containerData = container.getData();
console.log(result, containerData);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("assignWithDefault"),
        "expected assignWithDefault in output. output: {output}"
    );
    assert!(
        output.contains("Container"),
        "expected Container in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing in conditionals with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_conditional() {
    let source = r#"function processValue(input: string | null): string {
    if ((input ?? "default") === "default") {
        return "was nullish";
    }
    return input ?? "unreachable";
}

const value: number | undefined = undefined;
const condition = (value ?? 0) > 10;

const ternaryResult = (value ?? 0) > 5 ? "big" : "small";

function conditionalChain(a: boolean | null, b: boolean | undefined): boolean {
    return (a ?? false) && (b ?? true);
}

const chainResult = conditionalChain(null, undefined);
console.log(condition, ternaryResult, chainResult);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processValue"),
        "expected processValue in output. output: {output}"
    );
    assert!(
        output.contains("conditionalChain"),
        "expected conditionalChain in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with objects with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_objects() {
    let source = r#"interface Config {
    name?: string;
    count?: number;
    enabled?: boolean;
}

const config: Config | null = null;
const defaultConfig: Config = { name: "default", count: 0, enabled: true };

const finalConfig = config ?? defaultConfig;

function mergeConfigs(base: Config | undefined, override: Config | null): Config {
    return override ?? base ?? { name: "fallback", count: -1, enabled: false };
}

const merged = mergeConfigs(undefined, null);

const partialConfig: Config = {
    name: (config ?? defaultConfig).name ?? "unnamed",
    count: (config ?? defaultConfig).count ?? 0,
    enabled: (config ?? defaultConfig).enabled ?? false
};

console.log(finalConfig, merged, partialConfig);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("finalConfig"),
        "expected finalConfig in output. output: {output}"
    );
    assert!(
        output.contains("mergeConfigs"),
        "expected mergeConfigs in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for objects"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test nullish coalescing with optional chaining with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_with_optional_chaining() {
    let source = r#"interface User {
    name?: string;
    profile?: {
        email?: string;
        settings?: {
            theme?: string;
            language?: string;
        };
    };
}

function getUserTheme(user: User | null): string {
    return user?.profile?.settings?.theme ?? "light";
}

function getUserLanguage(user: User | undefined): string {
    return user?.profile?.settings?.language ?? "en";
}

const user: User | null = null;
const theme = user?.profile?.settings?.theme ?? "default-theme";
const language = user?.profile?.settings?.language ?? "default-lang";
const email = user?.profile?.email ?? "no-email@example.com";

class UserService {
    private user: User | null = null;

    getTheme(): string {
        return this.user?.profile?.settings?.theme ?? "system";
    }

    getDisplayName(): string {
        return this.user?.name ?? "Guest";
    }
}

const service = new UserService();
console.log(theme, language, email, service.getTheme());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getUserTheme"),
        "expected getUserTheme in output. output: {output}"
    );
    assert!(
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional chaining"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test for nullish coalescing patterns with ES5 target
#[test]
fn test_source_map_nullish_coalescing_es5_comprehensive() {
    let source = r#"// Comprehensive nullish coalescing scenarios
interface AppConfig {
    apiUrl?: string;
    timeout?: number;
    retries?: number;
    headers?: Record<string, string>;
    features?: {
        darkMode?: boolean;
        notifications?: boolean;
        analytics?: boolean;
    };
}

interface User {
    id: number;
    name?: string;
    email?: string;
    preferences?: AppConfig;
}

class ConfigManager {
    private defaultConfig: AppConfig = {
        apiUrl: "https://api.example.com",
        timeout: 5000,
        retries: 3,
        features: {
            darkMode: false,
            notifications: true,
            analytics: true
        }
    };

    private userConfig: AppConfig | null = null;

    setUserConfig(config: AppConfig | null): void {
        this.userConfig = config;
    }

    getApiUrl(): string {
        return this.userConfig?.apiUrl ?? this.defaultConfig.apiUrl ?? "https://fallback.com";
    }

    getTimeout(): number {
        return this.userConfig?.timeout ?? this.defaultConfig.timeout ?? 1000;
    }

    getRetries(): number {
        return this.userConfig?.retries ?? this.defaultConfig.retries ?? 0;
    }

    isDarkModeEnabled(): boolean {
        return this.userConfig?.features?.darkMode ?? this.defaultConfig.features?.darkMode ?? false;
    }

    areNotificationsEnabled(): boolean {
        return this.userConfig?.features?.notifications ?? this.defaultConfig.features?.notifications ?? true;
    }
}

class UserManager {
    private users: Map<number, User> = new Map();

    addUser(user: User): void {
        this.users.set(user.id, user);
    }

    getUserName(id: number): string {
        return this.users.get(id)?.name ?? "Unknown User";
    }

    getUserEmail(id: number): string {
        return this.users.get(id)?.email ?? "no-reply@example.com";
    }

    getUserApiUrl(id: number): string {
        const user = this.users.get(id);
        return user?.preferences?.apiUrl ?? "https://default-api.com";
    }
}

// Utility functions
function coalesce<T>(value: T | null | undefined, defaultValue: T): T {
    return value ?? defaultValue;
}

function coalesceMany<T>(...values: (T | null | undefined)[]): T | undefined {
    for (const value of values) {
        if (value !== null && value !== undefined) {
            return value;
        }
    }
    return undefined;
}

function getNestedValue<T>(
    obj: any,
    path: string[],
    defaultValue: T
): T {
    let current = obj;
    for (const key of path) {
        current = current?.[key];
        if (current === null || current === undefined) {
            return defaultValue;
        }
    }
    return current ?? defaultValue;
}

// Usage
const configManager = new ConfigManager();
const userManager = new UserManager();

userManager.addUser({ id: 1, name: "Alice" });
userManager.addUser({ id: 2, email: "bob@example.com" });

console.log(configManager.getApiUrl());
console.log(configManager.getTimeout());
console.log(configManager.isDarkModeEnabled());
console.log(userManager.getUserName(1));
console.log(userManager.getUserEmail(2));

const maybeValue: string | null = null;
const result = coalesce(maybeValue, "fallback");
const multiResult = coalesceMany<string>(null, undefined, "found", "ignored");

console.log(result, multiResult);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserManager"),
        "expected UserManager in output. output: {output}"
    );
    assert!(
        output.contains("coalesce"),
        "expected coalesce in output. output: {output}"
    );
    assert!(
        output.contains("coalesceMany"),
        "expected coalesceMany in output. output: {output}"
    );
    assert!(
        output.contains("getNestedValue"),
        "expected getNestedValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive nullish coalescing"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - LOGICAL ASSIGNMENT PATTERNS
// =============================================================================
// Tests for logical assignment patterns (&&=, ||=, ??=) with ES5 target to verify
// source maps work correctly with logical assignment transforms.

/// Test &&= logical AND assignment with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_and_assign() {
    let source = r#"let value1: string | null = "hello";
let value2: string | null = null;

value1 &&= "updated";
value2 &&= "updated";

function updateIfTruthy(input: string | null): string | null {
    let result = input;
    result &&= "modified";
    return result;
}

const result1 = updateIfTruthy("test");
const result2 = updateIfTruthy(null);
console.log(value1, value2, result1, result2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("value1"),
        "expected value1 in output. output: {output}"
    );
    assert!(
        output.contains("updateIfTruthy"),
        "expected updateIfTruthy in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for &&= assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test ||= logical OR assignment with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_or_assign() {
    let source = r#"let value1: string | null = null;
let value2: string | null = "existing";

value1 ||= "default";
value2 ||= "default";

function setDefault(input: string | null): string {
    let result: string | null = input;
    result ||= "fallback";
    return result;
}

const result1 = setDefault(null);
const result2 = setDefault("provided");
console.log(value1, value2, result1, result2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("value1"),
        "expected value1 in output. output: {output}"
    );
    assert!(
        output.contains("setDefault"),
        "expected setDefault in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ||= assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test ??= nullish coalescing assignment with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_nullish_assign() {
    let source = r#"let value1: string | null = null;
let value2: string | undefined = undefined;
let value3: string | null = "existing";

value1 ??= "default1";
value2 ??= "default2";
value3 ??= "default3";

function ensureValue(input: number | undefined): number {
    let result = input;
    result ??= 0;
    return result;
}

const result1 = ensureValue(undefined);
const result2 = ensureValue(42);
console.log(value1, value2, value3, result1, result2);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("value1"),
        "expected value1 in output. output: {output}"
    );
    assert!(
        output.contains("ensureValue"),
        "expected ensureValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ??= assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment with object properties with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_object_property() {
    let source = r#"interface Config {
    name: string | null;
    count: number | undefined;
    enabled: boolean;
}

const config: Config = {
    name: null,
    count: undefined,
    enabled: false
};

config.name ||= "default-name";
config.count ??= 0;
config.enabled &&= true;

function updateConfig(cfg: Config): void {
    cfg.name ??= "unnamed";
    cfg.count ||= 1;
}

updateConfig(config);
console.log(config);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("config"),
        "expected config in output. output: {output}"
    );
    assert!(
        output.contains("updateConfig"),
        "expected updateConfig in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object property"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment with element access with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_element_access() {
    let source = r#"const arr: (string | null)[] = [null, "existing", null];
const obj: Record<string, number | undefined> = { a: undefined, b: 10 };

arr[0] ||= "default0";
arr[1] ||= "default1";
arr[2] ??= "default2";

obj["a"] ??= 0;
obj["b"] &&= 20;
obj["c"] ??= 30;

function updateArray(items: (string | null)[], index: number): void {
    items[index] ??= "fallback";
}

updateArray(arr, 0);
console.log(arr, obj);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("arr"),
        "expected arr in output. output: {output}"
    );
    assert!(
        output.contains("updateArray"),
        "expected updateArray in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for element access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test chained logical assignments with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_chained() {
    let source = r#"let a: string | null = null;
let b: string | null = null;
let c: string | null = null;

a ||= "a-default";
b ??= a;
c &&= b;

interface ChainedConfig {
    primary: string | null;
    secondary: string | null;
    tertiary: string | null;
}

function chainedAssignments(cfg: ChainedConfig): void {
    cfg.primary ??= "primary-default";
    cfg.secondary ||= cfg.primary;
    cfg.tertiary &&= cfg.secondary;
}

const cfg: ChainedConfig = { primary: null, secondary: null, tertiary: "exists" };
chainedAssignments(cfg);
console.log(a, b, c, cfg);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("chainedAssignments"),
        "expected chainedAssignments in output. output: {output}"
    );
    assert!(
        output.contains("cfg"),
        "expected cfg in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained assignments"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment in function context with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_function_context() {
    let source = r#"function processWithDefaults(
    name: string | null,
    count: number | undefined,
    enabled: boolean
): { name: string; count: number; enabled: boolean } {
    let n = name;
    let c = count;
    let e = enabled;

    n ??= "anonymous";
    c ||= 1;
    e &&= true;

    return { name: n, count: c, enabled: e };
}

const arrowWithAssign = (val: string | null): string => {
    let result = val;
    result ??= "arrow-default";
    return result;
};

function nestedFunction(): string {
    let outer: string | null = null;

    function inner(): void {
        outer ??= "from-inner";
    }

    inner();
    return outer ?? "never";
}

console.log(processWithDefaults(null, undefined, true));
console.log(arrowWithAssign(null));
console.log(nestedFunction());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processWithDefaults"),
        "expected processWithDefaults in output. output: {output}"
    );
    assert!(
        output.contains("arrowWithAssign"),
        "expected arrowWithAssign in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function context"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment in class methods with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_class_methods() {
    let source = r#"class DataManager {
    private data: string | null = null;
    private count: number | undefined = undefined;
    private active: boolean = true;

    ensureData(): string {
        this.data ??= "default-data";
        return this.data;
    }

    ensureCount(): number {
        this.count ||= 0;
        return this.count;
    }

    updateActive(value: boolean): boolean {
        this.active &&= value;
        return this.active;
    }

    reset(): void {
        this.data = null;
        this.count = undefined;
        this.active = true;
    }
}

class CacheManager {
    private cache: Map<string, string | null> = new Map();

    getOrSet(key: string, defaultValue: string): string {
        let value = this.cache.get(key);
        value ??= defaultValue;
        this.cache.set(key, value);
        return value;
    }
}

const manager = new DataManager();
console.log(manager.ensureData());
console.log(manager.ensureCount());
console.log(manager.updateActive(false));

const cache = new CacheManager();
console.log(cache.getOrSet("key", "value"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("DataManager"),
        "expected DataManager in output. output: {output}"
    );
    assert!(
        output.contains("CacheManager"),
        "expected CacheManager in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment with side effects with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_side_effects() {
    let source = r#"let callCount = 0;

function getSideEffect(): string {
    callCount++;
    return "side-effect-value";
}

let value1: string | null = null;
let value2: string | null = "existing";

value1 ??= getSideEffect();
value2 ??= getSideEffect();

console.log("Call count:", callCount);

const obj = {
    _value: null as string | null,
    get value(): string | null {
        console.log("getter called");
        return this._value;
    },
    set value(v: string | null) {
        console.log("setter called");
        this._value = v;
    }
};

obj.value ??= "default";

function conditionalSideEffect(condition: boolean): string | null {
    if (condition) {
        return "truthy";
    }
    return null;
}

let sideEffectResult: string | null = null;
sideEffectResult ||= conditionalSideEffect(true);
console.log(value1, value2, sideEffectResult);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getSideEffect"),
        "expected getSideEffect in output. output: {output}"
    );
    assert!(
        output.contains("callCount"),
        "expected callCount in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for side effects"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test for logical assignment patterns with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_comprehensive() {
    let source = r#"// Comprehensive logical assignment scenarios
interface User {
    id: number;
    name: string | null;
    email: string | undefined;
    preferences: {
        theme: string | null;
        language: string | undefined;
        notifications: boolean;
    } | null;
}

class UserService {
    private users: Map<number, User> = new Map();

    createUser(id: number): User {
        const user: User = {
            id,
            name: null,
            email: undefined,
            preferences: null
        };
        this.users.set(id, user);
        return user;
    }

    ensureUserName(id: number): string {
        const user = this.users.get(id);
        if (user) {
            user.name ??= "Anonymous";
            return user.name;
        }
        return "Unknown";
    }

    ensureUserEmail(id: number, defaultEmail: string): string {
        const user = this.users.get(id);
        if (user) {
            user.email ||= defaultEmail;
            return user.email;
        }
        return defaultEmail;
    }

    ensurePreferences(id: number): void {
        const user = this.users.get(id);
        if (user) {
            user.preferences ??= {
                theme: null,
                language: undefined,
                notifications: true
            };
            user.preferences.theme ??= "light";
            user.preferences.language ||= "en";
            user.preferences.notifications &&= true;
        }
    }
}

class ConfigStore {
    private config: Record<string, any> = {};

    get<T>(key: string, defaultValue: T): T {
        let value = this.config[key] as T | undefined;
        value ??= defaultValue;
        return value;
    }

    set<T>(key: string, value: T): void {
        this.config[key] = value;
    }

    update<T>(key: string, updater: (val: T | undefined) => T): T {
        let current = this.config[key] as T | undefined;
        current ??= undefined as any;
        const updated = updater(current);
        this.config[key] = updated;
        return updated;
    }
}

// Utility functions
function ensureArray<T>(arr: T[] | null, defaultItems: T[]): T[] {
    let result = arr;
    result ??= defaultItems;
    return result;
}

function conditionalUpdate<T>(
    value: T | null,
    condition: boolean,
    newValue: T
): T | null {
    let result = value;
    if (condition) {
        result &&= newValue;
    } else {
        result ||= newValue;
    }
    return result;
}

// Usage
const userService = new UserService();
const user = userService.createUser(1);
console.log(userService.ensureUserName(1));
console.log(userService.ensureUserEmail(1, "default@example.com"));
userService.ensurePreferences(1);

const store = new ConfigStore();
console.log(store.get("theme", "dark"));
store.set("theme", "light");
console.log(store.get("theme", "dark"));

const items = ensureArray<string>(null, ["default"]);
console.log(items);

const updated = conditionalUpdate<string>("existing", true, "new");
console.log(updated);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        output.contains("ConfigStore"),
        "expected ConfigStore in output. output: {output}"
    );
    assert!(
        output.contains("ensureArray"),
        "expected ensureArray in output. output: {output}"
    );
    assert!(
        output.contains("conditionalUpdate"),
        "expected conditionalUpdate in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive logical assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - CLASS STATIC BLOCK PATTERNS (INIT ORDER, PRIVATE ACCESS)
// =============================================================================
// Tests for class static block patterns with ES5 target to verify source maps
// work correctly with static block transforms, focusing on init order and private access.

/// Test basic class static block with ES5 target
#[test]
fn test_source_map_static_block_es5_basic() {
    let source = r#"class Counter {
    static count: number;

    static {
        Counter.count = 0;
        console.log("Counter initialized");
    }

    static increment(): number {
        return ++Counter.count;
    }
}

console.log(Counter.increment());
console.log(Counter.increment());
console.log(Counter.count);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("increment"),
        "expected increment in output. output: {output}"
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

/// Test class static block initialization order with ES5 target
#[test]
fn test_source_map_static_block_es5_init_order() {
    let source = r#"const log: string[] = [];

class InitOrder {
    static a = (log.push("field a"), "a");

    static {
        log.push("block 1");
    }

    static b = (log.push("field b"), "b");

    static {
        log.push("block 2");
    }

    static c = (log.push("field c"), "c");

    static {
        log.push("block 3");
        console.log("Init order:", log.join(", "));
    }
}

console.log(InitOrder.a, InitOrder.b, InitOrder.c);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("InitOrder"),
        "expected InitOrder in output. output: {output}"
    );
    assert!(
        output.contains("log"),
        "expected log in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for init order"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test multiple class static blocks with ES5 target
#[test]
fn test_source_map_static_block_es5_multiple() {
    let source = r#"class MultiBlock {
    static config: Record<string, any> = {};

    static {
        MultiBlock.config.name = "app";
    }

    static {
        MultiBlock.config.version = "1.0.0";
    }

    static {
        MultiBlock.config.debug = false;
    }

    static {
        MultiBlock.config.features = ["a", "b", "c"];
        console.log("Config complete:", MultiBlock.config);
    }

    static getConfig(): Record<string, any> {
        return MultiBlock.config;
    }
}

console.log(MultiBlock.getConfig());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("MultiBlock"),
        "expected MultiBlock in output. output: {output}"
    );
    assert!(
        output.contains("getConfig"),
        "expected getConfig in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with private field access with ES5 target
#[test]
fn test_source_map_static_block_es5_private_field_access() {
    let source = r#"class PrivateFields {
    static #secret: string;
    static #counter: number;

    static {
        PrivateFields.#secret = "hidden-value";
        PrivateFields.#counter = 0;
    }

    static getSecret(): string {
        return PrivateFields.#secret;
    }

    static incrementCounter(): number {
        return ++PrivateFields.#counter;
    }

    static {
        console.log("Private fields initialized");
        console.log("Secret length:", PrivateFields.#secret.length);
    }
}

console.log(PrivateFields.getSecret());
console.log(PrivateFields.incrementCounter());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("PrivateFields"),
        "expected PrivateFields in output. output: {output}"
    );
    assert!(
        output.contains("getSecret"),
        "expected getSecret in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with private method access with ES5 target
#[test]
fn test_source_map_static_block_es5_private_method_access() {
    let source = r#"class PrivateMethods {
    static #initialize(): void {
        console.log("Initializing...");
    }

    static #validate(value: string): boolean {
        return value.length > 0;
    }

    static #format(value: string): string {
        return value.toUpperCase();
    }

    static {
        PrivateMethods.#initialize();
        const valid = PrivateMethods.#validate("test");
        console.log("Validation:", valid);
    }

    static process(input: string): string {
        if (PrivateMethods.#validate(input)) {
            return PrivateMethods.#format(input);
        }
        return "";
    }
}

console.log(PrivateMethods.process("hello"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("PrivateMethods"),
        "expected PrivateMethods in output. output: {output}"
    );
    assert!(
        output.contains("process"),
        "expected process in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private method access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with static field initialization with ES5 target
#[test]
fn test_source_map_static_block_es5_static_field_init() {
    let source = r#"class StaticInit {
    static readonly API_URL: string;
    static readonly TIMEOUT: number;
    static readonly HEADERS: Record<string, string>;

    static {
        const env = { api: "https://api.example.com", timeout: 5000 };
        StaticInit.API_URL = env.api;
        StaticInit.TIMEOUT = env.timeout;
        StaticInit.HEADERS = {
            "Content-Type": "application/json",
            "Accept": "application/json"
        };
    }

    static fetch(endpoint: string): Promise<any> {
        console.log(`Fetching ${StaticInit.API_URL}/${endpoint}`);
        return Promise.resolve({});
    }
}

console.log(StaticInit.API_URL);
console.log(StaticInit.TIMEOUT);
StaticInit.fetch("users");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("StaticInit"),
        "expected StaticInit in output. output: {output}"
    );
    assert!(
        output.contains("fetch"),
        "expected fetch in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static field init"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with computed property names with ES5 target
#[test]
fn test_source_map_static_block_es5_computed_props() {
    let source = r#"const KEY1 = "dynamicKey1";
const KEY2 = "dynamicKey2";

class ComputedProps {
    static [KEY1]: string;
    static [KEY2]: number;
    static computed: Record<string, any> = {};

    static {
        ComputedProps[KEY1] = "dynamic-value-1";
        ComputedProps[KEY2] = 42;
        ComputedProps.computed[KEY1] = "nested-dynamic";
    }

    static get(key: string): any {
        return ComputedProps.computed[key];
    }
}

console.log(ComputedProps[KEY1]);
console.log(ComputedProps[KEY2]);
console.log(ComputedProps.get(KEY1));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ComputedProps"),
        "expected ComputedProps in output. output: {output}"
    );
    assert!(
        output.contains("KEY1"),
        "expected KEY1 in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed props"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with async patterns with ES5 target
#[test]
fn test_source_map_static_block_es5_async_patterns() {
    let source = r#"class AsyncInit {
    static data: any;
    static ready: Promise<void>;

    static {
        AsyncInit.ready = (async () => {
            await new Promise(r => setTimeout(r, 100));
            AsyncInit.data = { loaded: true };
            console.log("Async init complete");
        })();
    }

    static async getData(): Promise<any> {
        await AsyncInit.ready;
        return AsyncInit.data;
    }
}

class LazyLoader {
    static #cache: Map<string, any> = new Map();

    static {
        console.log("LazyLoader initialized");
    }

    static async load(key: string): Promise<any> {
        if (!LazyLoader.#cache.has(key)) {
            const data = await fetch(key);
            LazyLoader.#cache.set(key, data);
        }
        return LazyLoader.#cache.get(key);
    }
}

AsyncInit.getData().then(console.log);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("AsyncInit"),
        "expected AsyncInit in output. output: {output}"
    );
    assert!(
        output.contains("LazyLoader"),
        "expected LazyLoader in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test static block with error handling with ES5 target
#[test]
fn test_source_map_static_block_es5_error_handling() {
    let source = r#"class ErrorHandling {
    static config: any;
    static error: Error | null = null;

    static {
        try {
            const raw = '{"valid": true}';
            ErrorHandling.config = JSON.parse(raw);
            console.log("Config loaded successfully");
        } catch (e) {
            ErrorHandling.error = e as Error;
            ErrorHandling.config = { fallback: true };
            console.error("Failed to load config:", e);
        } finally {
            console.log("Init complete");
        }
    }

    static isValid(): boolean {
        return ErrorHandling.error === null;
    }
}

class SafeInit {
    static #initialized = false;

    static {
        try {
            SafeInit.#initialized = true;
        } catch {
            SafeInit.#initialized = false;
        }
    }

    static isReady(): boolean {
        return SafeInit.#initialized;
    }
}

console.log(ErrorHandling.isValid());
console.log(SafeInit.isReady());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ErrorHandling"),
        "expected ErrorHandling in output. output: {output}"
    );
    assert!(
        output.contains("SafeInit"),
        "expected SafeInit in output. output: {output}"
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

/// Comprehensive test for class static block patterns with ES5 target
#[test]
fn test_source_map_static_block_es5_comprehensive() {
    let source = r#"// Comprehensive static block scenarios
class Registry {
    static #instances: Map<string, any> = new Map();
    static #initialized = false;

    static {
        Registry.#initialized = true;
        console.log("Registry initialized");
    }

    static register<T>(key: string, instance: T): void {
        Registry.#instances.set(key, instance);
    }

    static get<T>(key: string): T | undefined {
        return Registry.#instances.get(key) as T | undefined;
    }

    static isInitialized(): boolean {
        return Registry.#initialized;
    }
}

class ConfigManager {
    static readonly defaults: Record<string, any>;
    static #config: Record<string, any>;

    static {
        ConfigManager.defaults = {
            theme: "light",
            language: "en",
            timeout: 5000
        };
        ConfigManager.#config = { ...ConfigManager.defaults };
    }

    static get<T>(key: string): T {
        return ConfigManager.#config[key] as T;
    }

    static set<T>(key: string, value: T): void {
        ConfigManager.#config[key] = value;
    }

    static reset(): void {
        ConfigManager.#config = { ...ConfigManager.defaults };
    }
}

class EventBus {
    static #handlers: Map<string, Function[]> = new Map();
    static #eventCount = 0;

    static {
        EventBus.#handlers = new Map();
        console.log("EventBus ready");
    }

    static on(event: string, handler: Function): void {
        if (!EventBus.#handlers.has(event)) {
            EventBus.#handlers.set(event, []);
        }
        EventBus.#handlers.get(event)!.push(handler);
    }

    static emit(event: string, ...args: any[]): void {
        EventBus.#eventCount++;
        const handlers = EventBus.#handlers.get(event) || [];
        handlers.forEach(h => h(...args));
    }

    static getEventCount(): number {
        return EventBus.#eventCount;
    }
}

class DependencyInjector {
    static #container: Map<string, () => any> = new Map();

    static {
        DependencyInjector.#container.set("logger", () => console);
        DependencyInjector.#container.set("config", () => ConfigManager);
    }

    static {
        DependencyInjector.#container.set("events", () => EventBus);
        console.log("DI container configured");
    }

    static resolve<T>(key: string): T {
        const factory = DependencyInjector.#container.get(key);
        if (!factory) throw new Error(`No provider for ${key}`);
        return factory() as T;
    }

    static register<T>(key: string, factory: () => T): void {
        DependencyInjector.#container.set(key, factory);
    }
}

// Usage
Registry.register("app", { name: "MyApp" });
console.log(Registry.get("app"));
console.log(Registry.isInitialized());

ConfigManager.set("theme", "dark");
console.log(ConfigManager.get("theme"));
ConfigManager.reset();

EventBus.on("test", (msg: string) => console.log(msg));
EventBus.emit("test", "Hello!");
console.log(EventBus.getEventCount());

const logger = DependencyInjector.resolve<Console>("logger");
logger.log("DI working!");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Registry"),
        "expected Registry in output. output: {output}"
    );
    assert!(
        output.contains("ConfigManager"),
        "expected ConfigManager in output. output: {output}"
    );
    assert!(
        output.contains("EventBus"),
        "expected EventBus in output. output: {output}"
    );
    assert!(
        output.contains("DependencyInjector"),
        "expected DependencyInjector in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive static blocks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS - ASYNC/CLASS INTEGRATION PATTERNS
// =============================================================================
// Tests for async/class integration patterns with ES5 target to verify source maps
// work correctly with combined async and class transforms.

/// Test async method in derived class with super call
#[test]
fn test_source_map_async_class_integration_es5_derived_super_call() {
    let source = r#"class BaseService {
    protected baseUrl: string = "https://api.example.com";

    async fetchData(endpoint: string): Promise<any> {
        const response = await fetch(this.baseUrl + endpoint);
        return response.json();
    }

    async validate(data: any): Promise<boolean> {
        return data !== null && data !== undefined;
    }
}

class UserService extends BaseService {
    private userId: string;

    constructor(userId: string) {
        super();
        this.userId = userId;
    }

    async getUser(): Promise<any> {
        const data = await super.fetchData(`/users/${this.userId}`);
        const isValid = await super.validate(data);
        if (!isValid) {
            throw new Error("Invalid user data");
        }
        return data;
    }

    async updateUser(updates: any): Promise<any> {
        const currentData = await super.fetchData(`/users/${this.userId}`);
        const merged = { ...currentData, ...updates };
        return merged;
    }
}

const service = new UserService("123");
service.getUser().then(console.log);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("UserService"),
        "expected UserService in output. output: {output}"
    );
    assert!(
        output.contains("BaseService"),
        "expected BaseService in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async derived class with super"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test async arrow field initializer with ES5 target
#[test]
fn test_source_map_async_class_integration_es5_arrow_field_initializer() {
    let source = r#"class EventHandler {
    private name: string;
    private count: number = 0;

    // Async arrow as class field - captures 'this' lexically
    handleClick = async (event: any): Promise<void> => {
        this.count++;
        console.log(`${this.name} clicked ${this.count} times`);
        await this.processEvent(event);
    };

    handleHover = async (): Promise<string> => {
        return `Hovering over ${this.name}`;
    };

    handleSubmit = async (data: any): Promise<boolean> => {
        const result = await this.validate(data);
        if (result) {
            await this.save(data);
        }
        return result;
    };

    constructor(name: string) {
        this.name = name;
    }

    private async processEvent(event: any): Promise<void> {
        console.log("Processing:", event);
    }

    private async validate(data: any): Promise<boolean> {
        return data !== null;
    }

    private async save(data: any): Promise<void> {
        console.log("Saving:", data);
    }
}

const handler = new EventHandler("Button");
handler.handleClick({ type: "click" });"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("EventHandler"),
        "expected EventHandler in output. output: {output}"
    );
    assert!(
        output.contains("handleClick"),
        "expected handleClick in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async arrow field initializer"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
/// Validates that calling private methods (#method()) generates proper source mappings.
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

/// Test source map generation for nested conditional types in ES5 output.
/// Validates that deeply nested conditional patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_nested_es5() {
    let source = r#"// Nested conditional types
type DeepReadonly<T> = T extends (infer U)[]
    ? ReadonlyArray<DeepReadonly<U>>
    : T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;

// Type classification
type TypeName<T> = T extends string
    ? "string"
    : T extends number
    ? "number"
    : T extends boolean
    ? "boolean"
    : T extends undefined
    ? "undefined"
    : T extends Function
    ? "function"
    : "object";

// Flatten nested arrays
type Flatten<T> = T extends Array<infer U>
    ? U extends Array<any>
        ? Flatten<U>
        : U
    : T;

function getTypeName<T>(value: T): TypeName<T> {
    return typeof value as TypeName<T>;
}

function flatten<T>(arr: T[][]): Flatten<T[][]>[] {
    return arr.reduce((acc, val) => acc.concat(val), [] as Flatten<T[][]>[]);
}

const typeName = getTypeName("hello");
const flat = flatten([[1, 2], [3, 4]]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getTypeName"),
        "expected getTypeName function in output. output: {output}"
    );
    assert!(
        output.contains("flatten"),
        "expected flatten function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested conditional types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for conditional types in function returns in ES5 output.
/// Validates that conditional return types generate proper source mappings.
#[test]
fn test_source_map_conditional_type_function_return_es5() {
    let source = r#"// Conditional return based on input type
type StringOrNumberResult<T> = T extends string ? string[] : number[];

function process<T extends string | number>(
    input: T
): StringOrNumberResult<T> {
    if (typeof input === "string") {
        return input.split("") as StringOrNumberResult<T>;
    }
    return [input] as StringOrNumberResult<T>;
}

// Conditional async return
type AsyncResult<T> = T extends Promise<infer U> ? U : Promise<T>;

async function ensureAsync<T>(value: T): Promise<AsyncResult<T>> {
    if (value instanceof Promise) {
        return value as unknown as AsyncResult<T>;
    }
    return value as AsyncResult<T>;
}

// Method overload simulation with conditional
type MethodResult<T, K extends keyof T> = T[K] extends (...args: any[]) => infer R
    ? R
    : T[K];

function invoke<T extends object, K extends keyof T>(
    obj: T,
    key: K
): MethodResult<T, K> {
    const prop = obj[key];
    if (typeof prop === "function") {
        return (prop as Function).call(obj) as MethodResult<T, K>;
    }
    return prop as MethodResult<T, K>;
}

const strResult = process("hello");
const numResult = process(42);
const asyncVal = ensureAsync(123);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("process"),
        "expected process function in output. output: {output}"
    );
    assert!(
        output.contains("ensureAsync"),
        "expected ensureAsync function in output. output: {output}"
    );
    assert!(
        output.contains("invoke"),
        "expected invoke function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional function return types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for conditional types with unions in ES5 output.
/// Validates that union conditional patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_union_es5() {
    let source = r#"// Union in conditional check
type IsUnion<T, U = T> = T extends U
    ? [U] extends [T]
        ? false
        : true
    : never;

// Conditional with union result
type Result<T, E> = T extends Error ? { ok: false; error: E } : { ok: true; value: T };

// Union narrowing conditional
type UnwrapPromise<T> = T extends Promise<infer U>
    ? U
    : T extends PromiseLike<infer U>
    ? U
    : T;

// Handler type based on event
type EventHandler<T> = T extends "click"
    ? (e: MouseEvent) => void
    : T extends "keypress"
    ? (e: KeyboardEvent) => void
    : T extends "submit"
    ? (e: Event) => void
    : never;

function createHandler<T extends "click" | "keypress" | "submit">(
    eventType: T,
    handler: EventHandler<T>
): void {
    document.addEventListener(eventType, handler as EventListener);
}

function wrapResult<T>(value: T): Result<T, Error> {
    if (value instanceof Error) {
        return { ok: false, error: value } as Result<T, Error>;
    }
    return { ok: true, value } as Result<T, Error>;
}

const wrapped = wrapResult(42);
const errorWrapped = wrapResult(new Error("oops"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createHandler"),
        "expected createHandler function in output. output: {output}"
    );
    assert!(
        output.contains("wrapResult"),
        "expected wrapResult function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional type with union"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple conditional type expression patterns.
/// Tests infer, distributive, nested, and union conditional types together.
#[test]
fn test_source_map_conditional_type_es5_comprehensive() {
    let source = r#"// Complex conditional type utility library

// Extract constructor parameters
type ConstructorParameters<T> = T extends new (...args: infer P) => any ? P : never;

// Instance type from constructor
type InstanceType<T> = T extends new (...args: any[]) => infer R ? R : never;

// Readonly deep with conditional
type DeepPartial<T> = T extends object
    ? { [P in keyof T]?: DeepPartial<T[P]> }
    : T;

// Property types extraction
type PropertyType<T, K> = K extends keyof T ? T[K] : never;

// Function property keys
type FunctionKeys<T> = {
    [K in keyof T]: T[K] extends Function ? K : never;
}[keyof T];

// Non-function property keys
type DataKeys<T> = {
    [K in keyof T]: T[K] extends Function ? never : K;
}[keyof T];

// Class using conditional types
class TypedRegistry<T extends object> {
    private items: Map<string, T> = new Map();

    register(id: string, item: T): void {
        this.items.set(id, item);
    }

    get<K extends keyof T>(id: string, key: K): PropertyType<T, K> | undefined {
        const item = this.items.get(id);
        if (item) {
            return item[key] as PropertyType<T, K>;
        }
        return undefined;
    }

    update(id: string, partial: DeepPartial<T>): boolean {
        const item = this.items.get(id);
        if (item) {
            Object.assign(item, partial);
            return true;
        }
        return false;
    }

    callMethod<K extends FunctionKeys<T>>(
        id: string,
        method: K,
        ...args: T[K] extends (...args: infer P) => any ? P : never[]
    ): T[K] extends (...args: any[]) => infer R ? R : undefined {
        const item = this.items.get(id);
        if (item && typeof item[method] === "function") {
            return (item[method] as Function).apply(item, args);
        }
        return undefined as any;
    }
}

// Factory with conditional return
function createInstance<T extends new (...args: any[]) => any>(
    ctor: T,
    ...args: ConstructorParameters<T>
): InstanceType<T> {
    return new ctor(...args);
}

interface User {
    name: string;
    age: number;
    greet(): string;
}

const registry = new TypedRegistry<User>();
registry.register("user1", { name: "Alice", age: 30, greet: () => "Hello" });
const userName = registry.get("user1", "name");
registry.update("user1", { age: 31 });"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("TypedRegistry"),
        "expected TypedRegistry class in output. output: {output}"
    );
    assert!(
        output.contains("register"),
        "expected register method in output. output: {output}"
    );
    assert!(
        output.contains("update"),
        "expected update method in output. output: {output}"
    );
    assert!(
        output.contains("callMethod"),
        "expected callMethod method in output. output: {output}"
    );
    assert!(
        output.contains("createInstance"),
        "expected createInstance function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive conditional types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: IMPORT/EXPORT ALIASES
// =============================================================================

/// Test source map generation for named imports with aliases in ES5 output.
/// Validates that `import { foo as bar }` generates proper source mappings.
#[test]
fn test_source_map_import_named_alias_es5() {
    let source = r#"// Named import with alias
import { useState as useStateHook } from "react";
import { Component as ReactComponent, createElement as h } from "react";
import { map as arrayMap, filter as arrayFilter, reduce as arrayReduce } from "lodash";

// Using aliased imports
function MyComponent() {
    const [count, setCount] = useStateHook(0);
    return h("div", null, count);
}

const numbers = [1, 2, 3, 4, 5];
const doubled = arrayMap(numbers, (n: number) => n * 2);
const evens = arrayFilter(numbers, (n: number) => n % 2 === 0);
const sum = arrayReduce(numbers, (acc: number, n: number) => acc + n, 0);

console.log(doubled, evens, sum);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("MyComponent"),
        "expected MyComponent function in output. output: {output}"
    );
    assert!(
        output.contains("doubled"),
        "expected doubled variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for named import aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for named exports with aliases in ES5 output.
/// Validates that `export { foo as bar }` generates proper source mappings.
#[test]
fn test_source_map_export_named_alias_es5() {
    let source = r#"// Internal implementations
function internalAdd(a: number, b: number): number {
    return a + b;
}

function internalSubtract(a: number, b: number): number {
    return a - b;
}

const internalPI = 3.14159;
const internalE = 2.71828;

class InternalCalculator {
    add(a: number, b: number): number {
        return internalAdd(a, b);
    }

    subtract(a: number, b: number): number {
        return internalSubtract(a, b);
    }
}

// Export with aliases
export { internalAdd as add };
export { internalSubtract as subtract };
export { internalPI as PI, internalE as E };
export { InternalCalculator as Calculator };

// Also export with different alias
export { internalAdd as sum, internalSubtract as difference };"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("internalAdd"),
        "expected internalAdd function in output. output: {output}"
    );
    assert!(
        output.contains("InternalCalculator"),
        "expected InternalCalculator class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for named export aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for re-exports with aliases in ES5 output.
/// Validates that `export { foo as bar } from "module"` generates proper source mappings.
#[test]
fn test_source_map_reexport_alias_es5() {
    let source = r#"// Re-export with aliases from other modules
export { useState as useStateHook } from "react";
export { Component as ReactComponent } from "react";
export { map as lodashMap, filter as lodashFilter } from "lodash";

// Re-export default as named
export { default as axios } from "axios";
export { default as express } from "express";

// Mixed re-exports with and without aliases
export { readFile as readFileAsync, writeFile as writeFileAsync } from "fs/promises";

// Re-export everything with namespace alias handled separately
// export * as utils from "./utils";

// Local function that uses re-exports conceptually
function useLibraries(): void {
    console.log("Libraries configured");
}

export { useLibraries };"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("useLibraries"),
        "expected useLibraries function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for re-export aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for default import aliases in ES5 output.
/// Validates that `import MyAlias from "module"` generates proper source mappings.
#[test]
fn test_source_map_import_default_alias_es5() {
    let source = r#"// Default imports (which are essentially aliases for the default export)
import React from "react";
import Express from "express";
import Lodash from "lodash";

// Using default imports
const app = Express();
const element = React.createElement("div", null, "Hello");
const sorted = Lodash.sortBy([3, 1, 2]);

// Default import with named imports
import Axios, { AxiosResponse, AxiosError } from "axios";

async function fetchData(): Promise<AxiosResponse> {
    try {
        return await Axios.get("/api/data");
    } catch (error) {
        throw error as AxiosError;
    }
}

// Re-assigning default imports
const MyReact = React;
const MyExpress = Express;

console.log(app, element, sorted);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchData"),
        "expected fetchData function in output. output: {output}"
    );
    assert!(
        output.contains("MyReact"),
        "expected MyReact variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default import aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for namespace import aliases in ES5 output.
/// Validates that `import * as ns from "module"` generates proper source mappings.
#[test]
fn test_source_map_import_namespace_alias_es5() {
    let source = r#"// Namespace imports
import * as React from "react";
import * as ReactDOM from "react-dom";
import * as Lodash from "lodash";
import * as Utils from "./utils";

// Using namespace imports
const element = React.createElement("div", { className: "container" }, "Hello");
const root = ReactDOM.createRoot(document.getElementById("root")!);

// Destructuring from namespace
const { map, filter, reduce } = Lodash;
const { formatDate, parseDate } = Utils;

// Using destructured values
const doubled = map([1, 2, 3], (n: number) => n * 2);
const evens = filter([1, 2, 3, 4], (n: number) => n % 2 === 0);

// Aliasing namespace members
const lodashMap = Lodash.map;
const lodashFilter = Lodash.filter;

function renderApp(): void {
    root.render(element);
}

console.log(doubled, evens);
renderApp();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("renderApp"),
        "expected renderApp function in output. output: {output}"
    );
    assert!(
        output.contains("doubled"),
        "expected doubled variable in output. output: {output}"
    );
    assert!(
        output.contains("lodashMap"),
        "expected lodashMap variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for namespace import aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple import/export alias patterns.
/// Tests named, default, namespace imports and exports with various alias combinations.
#[test]
fn test_source_map_import_export_alias_es5_comprehensive() {
    let source = r#"// Comprehensive import/export alias patterns

// Namespace imports
import * as path from "path";
import * as fs from "fs";

// Default imports
import express from "express";
import cors from "cors";

// Named imports with aliases
import { readFile as readFileAsync, writeFile as writeFileAsync } from "fs/promises";
import { join as joinPath, resolve as resolvePath, dirname as getDirname } from "path";

// Mixed default and named with aliases
import axios, { AxiosInstance as HttpClient, AxiosResponse as HttpResponse } from "axios";

// Internal implementations
class ApiClient {
    private client: HttpClient;
    private basePath: string;

    constructor(baseUrl: string) {
        this.client = axios.create({ baseURL: baseUrl });
        this.basePath = resolvePath(getDirname(""), "api");
    }

    async get<T>(endpoint: string): Promise<HttpResponse<T>> {
        const fullPath = joinPath(this.basePath, endpoint);
        console.log(`Fetching from: ${fullPath}`);
        return this.client.get(endpoint);
    }

    async loadConfig(configPath: string): Promise<string> {
        const absolutePath = path.resolve(configPath);
        const content = await readFileAsync(absolutePath, "utf-8");
        return content;
    }

    async saveConfig(configPath: string, data: string): Promise<void> {
        const absolutePath = path.resolve(configPath);
        await writeFileAsync(absolutePath, data, "utf-8");
    }
}

// Create app with middleware
const app = express();
app.use(cors());

// Export with aliases
export { ApiClient as Client };
export { app as application };

// Re-export with aliases
export { readFileAsync as readFile, writeFileAsync as writeFile };
export { joinPath, resolvePath, getDirname };

// Export default with alias pattern
const defaultClient = new ApiClient("https://api.example.com");
export { defaultClient as default };"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ApiClient"),
        "expected ApiClient class in output. output: {output}"
    );
    assert!(
        output.contains("loadConfig"),
        "expected loadConfig method in output. output: {output}"
    );
    assert!(
        output.contains("saveConfig"),
        "expected saveConfig method in output. output: {output}"
    );
    assert!(
        output.contains("defaultClient"),
        "expected defaultClient variable in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive import/export aliases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: MAPPED TYPE EXPRESSIONS
// =============================================================================

/// Test source map generation for Partial<T> mapped type in ES5 output.
/// Validates that Partial utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_partial_es5() {
    let source = r#"// Custom Partial implementation
type MyPartial<T> = {
    [P in keyof T]?: T[P];
};

interface User {
    id: number;
    name: string;
    email: string;
    age: number;
}

// Function using Partial
function updateUser(user: User, updates: Partial<User>): User {
    return { ...user, ...updates };
}

function patchUser(user: User, patch: MyPartial<User>): User {
    return { ...user, ...patch };
}

// Creating partial objects
const fullUser: User = { id: 1, name: "Alice", email: "alice@example.com", age: 30 };
const partialUpdate: Partial<User> = { name: "Alicia" };
const updatedUser = updateUser(fullUser, partialUpdate);

// Nested partial
type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

interface Config {
    database: { host: string; port: number };
    cache: { enabled: boolean; ttl: number };
}

function mergeConfig(base: Config, override: DeepPartial<Config>): Config {
    return { ...base, ...override } as Config;
}

console.log(updatedUser);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("updateUser"),
        "expected updateUser function in output. output: {output}"
    );
    assert!(
        output.contains("patchUser"),
        "expected patchUser function in output. output: {output}"
    );
    assert!(
        output.contains("mergeConfig"),
        "expected mergeConfig function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Partial mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Required<T> mapped type in ES5 output.
/// Validates that Required utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_required_es5() {
    let source = r#"// Custom Required implementation
type MyRequired<T> = {
    [P in keyof T]-?: T[P];
};

interface PartialUser {
    id?: number;
    name?: string;
    email?: string;
}

// Function requiring all properties
function createUser(data: Required<PartialUser>): PartialUser {
    return {
        id: data.id,
        name: data.name,
        email: data.email
    };
}

function validateUser(data: MyRequired<PartialUser>): boolean {
    return data.id > 0 && data.name.length > 0 && data.email.includes("@");
}

// Builder pattern with Required
class UserBuilder {
    private data: Partial<PartialUser> = {};

    setId(id: number): this {
        this.data.id = id;
        return this;
    }

    setName(name: string): this {
        this.data.name = name;
        return this;
    }

    setEmail(email: string): this {
        this.data.email = email;
        return this;
    }

    build(): Required<PartialUser> {
        if (!this.data.id || !this.data.name || !this.data.email) {
            throw new Error("All fields required");
        }
        return this.data as Required<PartialUser>;
    }
}

const builder = new UserBuilder();
const user = builder.setId(1).setName("Bob").setEmail("bob@example.com").build();
console.log(validateUser(user));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createUser"),
        "expected createUser function in output. output: {output}"
    );
    assert!(
        output.contains("validateUser"),
        "expected validateUser function in output. output: {output}"
    );
    assert!(
        output.contains("UserBuilder"),
        "expected UserBuilder class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Required mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Readonly<T> mapped type in ES5 output.
/// Validates that Readonly utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_readonly_es5() {
    let source = r#"// Custom Readonly implementation
type MyReadonly<T> = {
    readonly [P in keyof T]: T[P];
};

interface MutableState {
    count: number;
    items: string[];
    lastUpdated: Date;
}

// Frozen state pattern
function freezeState<T extends object>(state: T): Readonly<T> {
    return Object.freeze({ ...state });
}

function getImmutableState(state: MutableState): MyReadonly<MutableState> {
    return state;
}

// Deep readonly
type DeepReadonly<T> = {
    readonly [P in keyof T]: T[P] extends object ? DeepReadonly<T[P]> : T[P];
};

interface AppState {
    user: { name: string; settings: { theme: string } };
    data: { items: number[] };
}

function getAppState(): DeepReadonly<AppState> {
    return {
        user: { name: "Alice", settings: { theme: "dark" } },
        data: { items: [1, 2, 3] }
    };
}

// Working with readonly
class StateManager {
    private state: MutableState = { count: 0, items: [], lastUpdated: new Date() };

    getState(): Readonly<MutableState> {
        return this.state;
    }

    increment(): void {
        this.state.count++;
        this.state.lastUpdated = new Date();
    }
}

const manager = new StateManager();
const readonlyState = manager.getState();
console.log(readonlyState.count);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("freezeState"),
        "expected freezeState function in output. output: {output}"
    );
    assert!(
        output.contains("getAppState"),
        "expected getAppState function in output. output: {output}"
    );
    assert!(
        output.contains("StateManager"),
        "expected StateManager class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Readonly mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Pick<T, K> mapped type in ES5 output.
/// Validates that Pick utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_pick_es5() {
    let source = r#"// Custom Pick implementation
type MyPick<T, K extends keyof T> = {
    [P in K]: T[P];
};

interface FullUser {
    id: number;
    name: string;
    email: string;
    password: string;
    createdAt: Date;
    updatedAt: Date;
}

// Pick specific properties
type PublicUser = Pick<FullUser, "id" | "name" | "email">;
type UserCredentials = MyPick<FullUser, "email" | "password">;

function getPublicProfile(user: FullUser): PublicUser {
    return {
        id: user.id,
        name: user.name,
        email: user.email
    };
}

function extractCredentials(user: FullUser): UserCredentials {
    return {
        email: user.email,
        password: user.password
    };
}

// Generic pick function
function pick<T extends object, K extends keyof T>(
    obj: T,
    keys: K[]
): Pick<T, K> {
    const result = {} as Pick<T, K>;
    for (const key of keys) {
        result[key] = obj[key];
    }
    return result;
}

const fullUser: FullUser = {
    id: 1,
    name: "Alice",
    email: "alice@example.com",
    password: "secret",
    createdAt: new Date(),
    updatedAt: new Date()
};

const publicUser = getPublicProfile(fullUser);
const picked = pick(fullUser, ["id", "name"]);
console.log(publicUser, picked);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getPublicProfile"),
        "expected getPublicProfile function in output. output: {output}"
    );
    assert!(
        output.contains("extractCredentials"),
        "expected extractCredentials function in output. output: {output}"
    );
    assert!(
        output.contains("pick"),
        "expected pick function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Pick mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Record<K, T> mapped type in ES5 output.
/// Validates that Record utility type generates proper source mappings.
#[test]
fn test_source_map_mapped_type_record_es5() {
    let source = r#"// Custom Record implementation
type MyRecord<K extends keyof any, T> = {
    [P in K]: T;
};

// Record with string keys
type UserRoles = Record<string, boolean>;
type CountryCode = "US" | "UK" | "CA" | "AU";
type CountryNames = Record<CountryCode, string>;

function createUserRoles(): UserRoles {
    return {
        admin: true,
        editor: false,
        viewer: true
    };
}

function getCountryNames(): CountryNames {
    return {
        US: "United States",
        UK: "United Kingdom",
        CA: "Canada",
        AU: "Australia"
    };
}

// Record with number keys
type IndexedData = Record<number, string>;

function createIndexedData(items: string[]): IndexedData {
    const result: IndexedData = {};
    items.forEach((item, index) => {
        result[index] = item;
    });
    return result;
}

// Nested Record
type NestedRecord = Record<string, Record<string, number>>;

function createNestedRecord(): NestedRecord {
    return {
        users: { count: 100, active: 50 },
        posts: { count: 500, published: 450 }
    };
}

// Generic record creator
function createRecord<K extends string, T>(
    keys: K[],
    value: T
): MyRecord<K, T> {
    const result = {} as MyRecord<K, T>;
    for (const key of keys) {
        result[key] = value;
    }
    return result;
}

const roles = createUserRoles();
const countries = getCountryNames();
const indexed = createIndexedData(["a", "b", "c"]);
console.log(roles, countries, indexed);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("createUserRoles"),
        "expected createUserRoles function in output. output: {output}"
    );
    assert!(
        output.contains("getCountryNames"),
        "expected getCountryNames function in output. output: {output}"
    );
    assert!(
        output.contains("createNestedRecord"),
        "expected createNestedRecord function in output. output: {output}"
    );
    assert!(
        output.contains("createRecord"),
        "expected createRecord function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Record mapped type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple mapped type patterns.
/// Tests Partial, Required, Readonly, Pick, Record, and custom mapped types together.
#[test]
fn test_source_map_mapped_type_es5_comprehensive() {
    let source = r#"// Comprehensive mapped type utility library

// Standard mapped types
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Record<K extends keyof any, T> = { [P in K]: T };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

// Custom mapped types
type Mutable<T> = { -readonly [P in keyof T]: T[P] };
type Nullable<T> = { [P in keyof T]: T[P] | null };
type NonNullableProps<T> = { [P in keyof T]: NonNullable<T[P]> };

// Key remapping
type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};

type Setters<T> = {
    [K in keyof T as `set${Capitalize<string & K>}`]: (value: T[K]) => void;
};

// Entity interface
interface Entity {
    id: number;
    name: string;
    createdAt: Date;
    updatedAt: Date | null;
}

// Repository using mapped types
class Repository<T extends Entity> {
    private items: Map<number, T> = new Map();

    create(data: Omit<T, "id" | "createdAt" | "updatedAt">): T {
        const now = new Date();
        const id = this.items.size + 1;
        const entity = {
            ...data,
            id,
            createdAt: now,
            updatedAt: null
        } as T;
        this.items.set(id, entity);
        return entity;
    }

    update(id: number, data: Partial<Omit<T, "id" | "createdAt">>): T | undefined {
        const entity = this.items.get(id);
        if (entity) {
            const updated = { ...entity, ...data, updatedAt: new Date() };
            this.items.set(id, updated);
            return updated;
        }
        return undefined;
    }

    findById(id: number): Readonly<T> | undefined {
        return this.items.get(id);
    }

    findAll(): ReadonlyArray<Readonly<T>> {
        return Array.from(this.items.values());
    }

    getFields<K extends keyof T>(id: number, fields: K[]): Pick<T, K> | undefined {
        const entity = this.items.get(id);
        if (entity) {
            const result = {} as Pick<T, K>;
            for (const field of fields) {
                result[field] = entity[field];
            }
            return result;
        }
        return undefined;
    }
}

// Form state using mapped types
type FormState<T> = {
    values: T;
    errors: Partial<Record<keyof T, string>>;
    touched: Partial<Record<keyof T, boolean>>;
    dirty: boolean;
};

function createFormState<T>(initial: T): FormState<T> {
    return {
        values: initial,
        errors: {},
        touched: {},
        dirty: false
    };
}

interface User extends Entity {
    email: string;
    role: "admin" | "user";
}

const userRepo = new Repository<User>();
const newUser = userRepo.create({ name: "Alice", email: "alice@example.com", role: "user" });
const formState = createFormState({ name: "", email: "" });
console.log(newUser, formState);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("Repository"),
        "expected Repository class in output. output: {output}"
    );
    assert!(
        output.contains("create"),
        "expected create method in output. output: {output}"
    );
    assert!(
        output.contains("update"),
        "expected update method in output. output: {output}"
    );
    assert!(
        output.contains("findById"),
        "expected findById method in output. output: {output}"
    );
    assert!(
        output.contains("getFields"),
        "expected getFields method in output. output: {output}"
    );
    assert!(
        output.contains("createFormState"),
        "expected createFormState function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive mapped types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: UTILITY TYPES
// =============================================================================

/// Test source map generation for ReturnType<T> utility type in ES5 output.
/// Validates that ReturnType extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_return_type_es5() {
    let source = r#"// Custom ReturnType implementation
type MyReturnType<T extends (...args: any[]) => any> = T extends (...args: any[]) => infer R ? R : never;

// Functions to extract return types from
function getString(): string {
    return "hello";
}

function getNumber(): number {
    return 42;
}

async function getAsyncData(): Promise<{ id: number; name: string }> {
    return { id: 1, name: "test" };
}

function getCallback(): (x: number) => boolean {
    return (x) => x > 0;
}

// Using ReturnType
type StringResult = ReturnType<typeof getString>;
type NumberResult = ReturnType<typeof getNumber>;
type AsyncResult = ReturnType<typeof getAsyncData>;
type CallbackResult = MyReturnType<typeof getCallback>;

// Functions that use extracted types
function processString(value: StringResult): void {
    console.log(value.toUpperCase());
}

function processNumber(value: NumberResult): void {
    console.log(value.toFixed(2));
}

// Generic wrapper using ReturnType
function wrapResult<T extends (...args: any[]) => any>(
    fn: T
): { result: ReturnType<T>; timestamp: Date } | null {
    try {
        return { result: fn(), timestamp: new Date() };
    } catch {
        return null;
    }
}

const wrapped = wrapResult(getString);
processString("test");
processNumber(123);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getString"),
        "expected getString function in output. output: {output}"
    );
    assert!(
        output.contains("wrapResult"),
        "expected wrapResult function in output. output: {output}"
    );
    assert!(
        output.contains("processString"),
        "expected processString function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ReturnType utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for Parameters<T> utility type in ES5 output.
/// Validates that Parameters extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_parameters_es5() {
    let source = r#"// Custom Parameters implementation
type MyParameters<T extends (...args: any[]) => any> = T extends (...args: infer P) => any ? P : never;

// Functions with various parameter signatures
function simpleFunc(a: string, b: number): void {
    console.log(a, b);
}

function optionalFunc(required: string, optional?: number): boolean {
    return optional !== undefined;
}

function restFunc(first: string, ...rest: number[]): number {
    return rest.reduce((sum, n) => sum + n, 0);
}

function complexFunc(
    config: { host: string; port: number },
    callback: (err: Error | null, result: string) => void
): void {
    callback(null, `${config.host}:${config.port}`);
}

// Using Parameters
type SimpleParams = Parameters<typeof simpleFunc>;
type OptionalParams = Parameters<typeof optionalFunc>;
type RestParams = MyParameters<typeof restFunc>;
type ComplexParams = Parameters<typeof complexFunc>;

// Function that forwards parameters
function forward<T extends (...args: any[]) => any>(
    fn: T,
    ...args: Parameters<T>
): ReturnType<T> {
    return fn(...args);
}

// Partial application using Parameters
function partial<T extends (...args: any[]) => any>(
    fn: T,
    firstArg: Parameters<T>[0]
): (...rest: Parameters<T> extends [any, ...infer R] ? R : never[]) => ReturnType<T> {
    return (...rest) => fn(firstArg, ...rest);
}

const forwardedResult = forward(simpleFunc, "hello", 42);
const partialSimple = partial(simpleFunc, "fixed");
partialSimple(123);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("simpleFunc"),
        "expected simpleFunc function in output. output: {output}"
    );
    assert!(
        output.contains("forward"),
        "expected forward function in output. output: {output}"
    );
    assert!(
        output.contains("partial"),
        "expected partial function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Parameters utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for ConstructorParameters<T> utility type in ES5 output.
/// Validates that ConstructorParameters extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_constructor_params_es5() {
    let source = r#"// Custom ConstructorParameters implementation
type MyConstructorParameters<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: infer P) => any ? P : never;

// Classes with various constructor signatures
class SimpleClass {
    constructor(public name: string, public age: number) {}
}

class ConfigurableClass {
    constructor(
        public config: { host: string; port: number },
        public options?: { timeout?: number; retries?: number }
    ) {}
}

class VariadicClass {
    public items: string[];
    constructor(first: string, ...rest: string[]) {
        this.items = [first, ...rest];
    }
}

// Using ConstructorParameters
type SimpleCtorParams = ConstructorParameters<typeof SimpleClass>;
type ConfigCtorParams = ConstructorParameters<typeof ConfigurableClass>;
type VariadicCtorParams = MyConstructorParameters<typeof VariadicClass>;

// Factory function using ConstructorParameters
function createInstance<T extends new (...args: any[]) => any>(
    ctor: T,
    ...args: ConstructorParameters<T>
): InstanceType<T> {
    return new ctor(...args);
}

// Builder pattern with ConstructorParameters
class Factory<T extends new (...args: any[]) => any> {
    private args: ConstructorParameters<T> | null = null;

    constructor(private ctor: T) {}

    withArgs(...args: ConstructorParameters<T>): this {
        this.args = args;
        return this;
    }

    build(): InstanceType<T> {
        if (!this.args) throw new Error("Args not set");
        return new this.ctor(...this.args);
    }
}

const simple = createInstance(SimpleClass, "Alice", 30);
const factory = new Factory(ConfigurableClass);
const configured = factory.withArgs({ host: "localhost", port: 8080 }).build();
console.log(simple, configured);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("SimpleClass"),
        "expected SimpleClass in output. output: {output}"
    );
    assert!(
        output.contains("createInstance"),
        "expected createInstance function in output. output: {output}"
    );
    assert!(
        output.contains("Factory"),
        "expected Factory class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ConstructorParameters utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for InstanceType<T> utility type in ES5 output.
/// Validates that InstanceType extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_instance_type_es5() {
    let source = r#"// Custom InstanceType implementation
type MyInstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : never;

// Various classes
class User {
    constructor(public id: number, public name: string) {}

    greet(): string {
        return `Hello, ${this.name}`;
    }
}

class Product {
    constructor(
        public sku: string,
        public price: number,
        public inStock: boolean
    ) {}

    getDisplayPrice(): string {
        return `$${this.price.toFixed(2)}`;
    }
}

abstract class Entity {
    abstract getId(): string;
}

class ConcreteEntity extends Entity {
    constructor(private id: string) {
        super();
    }

    getId(): string {
        return this.id;
    }
}

// Using InstanceType
type UserInstance = InstanceType<typeof User>;
type ProductInstance = InstanceType<typeof Product>;
type EntityInstance = MyInstanceType<typeof ConcreteEntity>;

// Registry using InstanceType
class Registry<T extends new (...args: any[]) => any> {
    private instances: Map<string, InstanceType<T>> = new Map();

    register(key: string, instance: InstanceType<T>): void {
        this.instances.set(key, instance);
    }

    get(key: string): InstanceType<T> | undefined {
        return this.instances.get(key);
    }

    getAll(): InstanceType<T>[] {
        return Array.from(this.instances.values());
    }
}

const userRegistry = new Registry<typeof User>();
userRegistry.register("user1", new User(1, "Alice"));
userRegistry.register("user2", new User(2, "Bob"));

const users = userRegistry.getAll();
console.log(users.map(u => u.greet()));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("User"),
        "expected User class in output. output: {output}"
    );
    assert!(
        output.contains("Product"),
        "expected Product class in output. output: {output}"
    );
    assert!(
        output.contains("Registry"),
        "expected Registry class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for InstanceType utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for ThisParameterType<T> utility type in ES5 output.
/// Validates that ThisParameterType extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_this_parameter_es5() {
    let source = r#"// Custom ThisParameterType implementation
type MyThisParameterType<T> = T extends (this: infer U, ...args: any[]) => any ? U : unknown;

// Custom OmitThisParameter implementation
type MyOmitThisParameter<T> = unknown extends ThisParameterType<T>
    ? T
    : T extends (...args: infer A) => infer R
    ? (...args: A) => R
    : T;

// Functions with explicit this parameter
function greet(this: { name: string }): string {
    return `Hello, ${this.name}`;
}

function calculate(this: { multiplier: number }, value: number): number {
    return value * this.multiplier;
}

function processItems(
    this: { prefix: string },
    items: string[]
): string[] {
    return items.map(item => `${this.prefix}: ${item}`);
}

// Using ThisParameterType
type GreetThis = ThisParameterType<typeof greet>;
type CalculateThis = ThisParameterType<typeof calculate>;
type ProcessThis = MyThisParameterType<typeof processItems>;

// Binding functions with correct this
function bindThis<T, A extends any[], R>(
    fn: (this: T, ...args: A) => R,
    thisArg: T
): (...args: A) => R {
    return fn.bind(thisArg);
}

const context = { name: "Alice", multiplier: 2, prefix: "Item" };
const boundGreet = bindThis(greet, context);
const boundCalculate = bindThis(calculate, context);

// Method extraction with this handling
class Counter {
    count = 0;

    increment(this: Counter): void {
        this.count++;
    }

    getCount(this: Counter): number {
        return this.count;
    }
}

const counter = new Counter();
const incrementFn: MyOmitThisParameter<typeof counter.increment> = counter.increment.bind(counter);
incrementFn();

console.log(boundGreet(), boundCalculate(5), counter.getCount());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("greet"),
        "expected greet function in output. output: {output}"
    );
    assert!(
        output.contains("bindThis"),
        "expected bindThis function in output. output: {output}"
    );
    assert!(
        output.contains("Counter"),
        "expected Counter class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ThisParameterType utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple utility type patterns.
/// Tests ReturnType, Parameters, ConstructorParameters, InstanceType, and ThisParameterType together.
#[test]
fn test_source_map_utility_type_es5_comprehensive() {
    let source = r#"// Comprehensive utility type patterns

// All utility types defined
type ReturnType<T extends (...args: any) => any> = T extends (...args: any) => infer R ? R : never;
type Parameters<T extends (...args: any) => any> = T extends (...args: infer P) => any ? P : never;
type ConstructorParameters<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: infer P) => any ? P : never;
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : never;
type ThisParameterType<T> = T extends (this: infer U, ...args: any[]) => any ? U : unknown;
type OmitThisParameter<T> = unknown extends ThisParameterType<T>
    ? T
    : T extends (...args: infer A) => infer R ? (...args: A) => R : T;

// Service class using utility types
class ApiService {
    private baseUrl: string;

    constructor(baseUrl: string, private timeout: number = 5000) {
        this.baseUrl = baseUrl;
    }

    async fetch<T>(endpoint: string): Promise<T> {
        const response = await fetch(`${this.baseUrl}${endpoint}`);
        return response.json();
    }

    post<T, R>(endpoint: string, data: T): Promise<R> {
        return fetch(`${this.baseUrl}${endpoint}`, {
            method: "POST",
            body: JSON.stringify(data)
        }).then(r => r.json());
    }
}

// Dependency injection container using utility types
class Container {
    private factories: Map<string, (...args: any[]) => any> = new Map();
    private instances: Map<string, any> = new Map();

    register<T extends new (...args: any[]) => any>(
        key: string,
        ctor: T,
        ...args: ConstructorParameters<T>
    ): void {
        this.factories.set(key, () => new ctor(...args));
    }

    resolve<T extends new (...args: any[]) => any>(key: string): InstanceType<T> {
        if (!this.instances.has(key)) {
            const factory = this.factories.get(key);
            if (factory) {
                this.instances.set(key, factory());
            }
        }
        return this.instances.get(key);
    }
}

// Function composition using Parameters and ReturnType
function compose<
    F extends (...args: any[]) => any,
    G extends (arg: ReturnType<F>) => any
>(f: F, g: G): (...args: Parameters<F>) => ReturnType<G> {
    return (...args) => g(f(...args));
}

// Method decorator factory using utility types
function logMethod<T extends (...args: any[]) => any>(
    target: any,
    propertyKey: string,
    descriptor: TypedPropertyDescriptor<T>
): void {
    const original = descriptor.value!;
    descriptor.value = function(this: ThisParameterType<T>, ...args: Parameters<T>): ReturnType<T> {
        console.log(`Calling ${propertyKey} with`, args);
        return original.apply(this, args);
    } as T;
}

// Event handler with proper typing
interface EventMap {
    click: { x: number; y: number };
    keypress: { key: string; code: number };
    submit: { data: Record<string, string> };
}

class EventEmitter<T extends Record<string, any>> {
    private handlers: Map<keyof T, ((event: any) => void)[]> = new Map();

    on<K extends keyof T>(
        event: K,
        handler: (event: T[K]) => void
    ): void {
        if (!this.handlers.has(event)) {
            this.handlers.set(event, []);
        }
        this.handlers.get(event)!.push(handler);
    }

    emit<K extends keyof T>(event: K, data: T[K]): void {
        const handlers = this.handlers.get(event) || [];
        handlers.forEach(h => h(data));
    }
}

// Usage
const container = new Container();
container.register("api", ApiService, "https://api.example.com", 3000);
const api = container.resolve<typeof ApiService>("api");

const double = (x: number) => x * 2;
const stringify = (x: number) => x.toString();
const doubleAndStringify = compose(double, stringify);

const emitter = new EventEmitter<EventMap>();
emitter.on("click", (e) => console.log(e.x, e.y));
emitter.emit("click", { x: 10, y: 20 });

console.log(api, doubleAndStringify(21));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("ApiService"),
        "expected ApiService class in output. output: {output}"
    );
    assert!(
        output.contains("Container"),
        "expected Container class in output. output: {output}"
    );
    assert!(
        output.contains("compose"),
        "expected compose function in output. output: {output}"
    );
    assert!(
        output.contains("EventEmitter"),
        "expected EventEmitter class in output. output: {output}"
    );
    assert!(
        output.contains("register"),
        "expected register method in output. output: {output}"
    );
    assert!(
        output.contains("resolve"),
        "expected resolve method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive utility types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
