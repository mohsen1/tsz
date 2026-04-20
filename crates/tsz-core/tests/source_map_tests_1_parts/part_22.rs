#[test]
fn test_source_map_generator_multiple_yields() {
    // Test generator function with multiple yield statements
    let source = r#"function* multiYield() {
    yield 1;
    yield 2;
    yield 3;
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

    // Verify output contains generator function name
    assert!(
        output.contains("multiYield"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multi-yield generator"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_yield_in_loop() {
    // Test generator with yield inside a loop
    let source = r#"function* loopGenerator(max: number) {
    for (let i = 0; i < max; i++) {
        yield i;
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

    // Verify output contains generator function name
    assert!(
        output.contains("loopGenerator"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for loop generator"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_yield_delegation() {
    // Test generator with yield* (delegation)
    let source = r#"function* inner() {
    yield 1;
    yield 2;
}
function* outer() {
    yield* inner();
    yield 3;
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

    // Verify output contains both generator function names
    assert!(
        output.contains("inner") && output.contains("outer"),
        "expected output to contain both generator function names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for yield delegation"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_with_return() {
    // Test generator function with return value
    let source = r#"function* generatorWithReturn() {
    yield 1;
    yield 2;
    return "done";
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

    // Verify output contains generator function name
    assert!(
        output.contains("generatorWithReturn"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator with return"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_if_statement_brace_on_next_line_normalized() {
    // tsc normalizes `if (cond)\n{` to `if (cond) {` — the opening brace of a block
    // always goes on the same line as the if, regardless of source formatting.
    let source = r#"var i = 10;
if (i == 10)
{
    i++;
}
else if (i == 20) {
    i--;
} else {
    i--;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ESNext,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.take_output();

    // The brace should be on the same line as `if (i == 10)`
    assert!(
        output.contains("if (i == 10) {"),
        "Expected 'if (i == 10) {{' but got:\n{output}"
    );
    // Should NOT have the brace on its own line
    assert!(
        !output.contains("if (i == 10)\n{"),
        "Brace should not be on its own line:\n{output}"
    );
}

#[test]
fn test_interface_comment_erased_with_declaration() {
    // tsc erases leading comments of erased declarations (interface, type alias).
    // The `// Interface` comment should not appear in the output.
    let source = r#"// Interface
interface IPoint {
    getDist(): number;
}

// Module
namespace Shapes {
    var a = 10;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ESNext,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.take_output();

    // The interface comment should be erased
    assert!(
        !output.contains("// Interface"),
        "Interface comment should be erased with the interface declaration:\n{output}"
    );
    // The module comment should be preserved
    assert!(
        output.contains("// Module"),
        "Module comment should be preserved:\n{output}"
    );
}

#[test]
fn test_static_member_comment_preserved_after_class() {
    // When a static property initializer is moved outside the class body,
    // its leading comment should move with it.
    let source = r#"class Point {
    constructor(public x: number, public y: number) { }
    // Static member
    static origin = new Point(0, 0);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.take_output();

    // The static member comment should appear before the static init
    assert!(
        output.contains("// Static member\nPoint.origin"),
        "Static member comment should be preserved before the initialization.\nOutput:\n{output}"
    );
}

#[test]
fn test_shorthand_property_default_value_emitted() {
    // Object destructuring assignment with default value: { name = "noName" }
    let source = r#"let robots: { name: string }[] = [];
for ({ name = "noName" } of robots) {
    console.log(name);
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.take_output();

    assert!(
        output.contains(r#"name = "noName""#),
        "Shorthand property default value should be emitted.\nOutput:\n{output}"
    );
}

#[test]
fn test_non_exported_inner_namespace_no_parent_assignment() {
    // Non-exported inner namespace should not be assigned to parent
    let source = r#"namespace M {
    export namespace Exported {
        export let x = 1;
    }
    namespace NotExported {
        export let y = 2;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.take_output();

    // Exported namespace should have parent assignment
    assert!(
        output.contains("M.Exported"),
        "Exported namespace should be assigned to parent.\nOutput:\n{output}"
    );
    // Non-exported namespace should NOT have parent assignment
    assert!(
        !output.contains("M.NotExported"),
        "Non-exported namespace should NOT be assigned to parent.\nOutput:\n{output}"
    );
}

#[test]
fn test_detached_block_comment_preserved_before_erased_declaration() {
    // tsc preserves file-level block comments (copyright/license headers)
    // even when the first statement is an erased declaration (declare var,
    // interface, type alias). Comments separated by a blank line from the
    // first declaration are "detached" and always preserved.
    let source = r#"/*
 * Copyright notice
 */

declare var process: any;
declare var console: any;

var x = 1;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ESNext,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.take_output();

    // Block comment (detached by blank line) should be preserved
    assert!(
        output.contains("Copyright notice"),
        "Detached block comment should be preserved:\n{output}"
    );
    // The actual code should still be emitted
    assert!(
        output.contains("var x = 1;"),
        "Code after erased declarations should be emitted:\n{output}"
    );
}

