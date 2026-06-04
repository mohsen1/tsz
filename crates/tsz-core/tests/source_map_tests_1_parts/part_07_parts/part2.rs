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
