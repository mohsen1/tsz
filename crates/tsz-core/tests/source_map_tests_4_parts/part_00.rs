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

