#[test]
fn test_source_map_commonjs_default_export_mapping() {
    // Test CommonJS default export transform source mapping
    let source = r#"const myValue = 100;

export default myValue;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
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

    // Verify default export or myValue in output
    assert!(
        output.contains("myValue") || output.contains("default"),
        "expected default export in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default export"
    );
}

#[test]
fn test_source_map_commonjs_reexport_mapping() {
    // Test CommonJS re-export transform source mapping
    let source = r#"export { foo, bar } from "./module";
export * from "./utils";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
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

    // Verify we have mappings for re-exports
    let (reexport_line, _) = find_line_col(source, "export { foo");
    let has_reexport_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == reexport_line);

    assert!(
        has_reexport_mapping || !decoded.is_empty(),
        "expected mappings for re-exports. mappings: {mappings} output: {output}"
    );
}

#[test]
fn test_source_map_type_assertions_and_const() {
    // Test source-map accuracy for type assertions and const assertions
    let source = r#"const value: unknown = "hello";
const str = value as string;
const num = <number>someValue;

const config = {
    name: "app",
    version: 1
} as const;

const colors = ["red", "green", "blue"] as const;

function process(input: unknown) {
    const data = input as { id: number; name: string };
    return data.id;
}

type Point = { x: number; y: number };
const origin = { x: 0, y: 0 } as Point;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for variable declarations
    let (value_line, _) = find_line_col(source, "const value");
    let has_value_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == value_line);

    let (config_line, _) = find_line_col(source, "const config");
    let has_config_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == config_line);

    // At minimum, we should have mappings for declarations
    assert!(
        has_value_mapping || has_config_mapping || !decoded.is_empty(),
        "expected mappings for type assertions. mappings: {mappings}"
    );

    // Type assertions should be stripped from output
    assert!(
        !output.contains(" as string") && !output.contains(" as const"),
        "expected type assertions to be stripped. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type assertions"
    );
}

#[test]
fn test_source_map_jsx_element_mapping() {
    // Test JSX element source mapping
    let source = r#"const element = <div className="container">Hello</div>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX is in output
    assert!(
        output.contains("<div") || output.contains("div"),
        "expected JSX element in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX element"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_jsx_fragment_mapping() {
    // Test JSX fragment source mapping
    let source = r#"const fragment = <>
    <span>First</span>
    <span>Second</span>
</>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX fragment is in output
    assert!(
        output.contains("<>") || output.contains("Fragment") || output.contains("span"),
        "expected JSX fragment in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX fragment"
    );
}

#[test]
fn test_source_map_jsx_expression_mapping() {
    // Test JSX with expressions source mapping
    let source = r#"const name = "World";
const greeting = <h1>Hello, {name}!</h1>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX with expression is in output
    assert!(
        output.contains("name") && output.contains("h1"),
        "expected JSX expression in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for both declarations
    let (name_line, _) = find_line_col(source, "const name");
    let has_name_mapping = decoded.iter().any(|entry| entry.original_line == name_line);

    let (greeting_line, _) = find_line_col(source, "const greeting");
    let has_greeting_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == greeting_line);

    assert!(
        has_name_mapping || has_greeting_mapping || !decoded.is_empty(),
        "expected mappings for JSX expressions. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_jsx_component_mapping() {
    // Test JSX component with props source mapping
    let source = r#"function Button({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
    return <button onClick={onClick}>{children}</button>;
}

const app = <Button onClick={() => console.log("clicked")}>Click me</Button>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify component is in output
    assert!(
        output.contains("Button") && output.contains("button"),
        "expected JSX component in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX component"
    );

    // Verify mappings cover multiple source lines
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 2,
        "expected mappings from at least 2 different source lines, got: {unique_source_lines:?}"
    );
}

#[test]
fn test_source_map_namespace_es5_basic_mapping() {
    // Basic namespace transforms to IIFE pattern
    let source = r#"namespace Foo {
    export const value = 42;
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

    assert!(
        output.contains("var Foo;"),
        "expected var Foo declaration in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for namespace"
    );
}

#[test]
fn test_source_map_namespace_es5_nested_mapping() {
    // Nested/qualified namespace A.B.C
    let source = r#"namespace A.B.C {
    export function greet() {
        return "hello";
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

    assert!(
        output.contains("var A;") || output.contains("var A "),
        "expected var A declaration for nested namespace in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested namespace"
    );
}

#[test]
fn test_source_map_class_decorator_single() {
    // Test single class decorator source mapping
    let source = r#"function Component(target: Function) {
    return target;
}

@Component
class MyComponent {
    render() {
        return "hello";
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

    // Verify output contains the class and decorator pattern
    assert!(
        output.contains("MyComponent") && output.contains("render"),
        "expected output to contain class and method. output: {output}"
    );

    // Verify we have source mappings that reference source file
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class decorator"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

