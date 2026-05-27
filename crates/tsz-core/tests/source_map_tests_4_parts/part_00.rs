#[test]
fn test_source_map_jsx_es5_spread_attributes() {
    // Test JSX spread attributes transformation
    let source = r#"const props = { id: "main", className: "container" };
const element = <div {...props} data-testid="test">Content</div>;"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
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

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
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

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
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

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
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

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
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

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
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

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
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

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
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

