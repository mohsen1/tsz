#[test]
fn test_source_map_generator_yield_in_loop() {
    // Test generator with yield inside a loop
    let source = r#"function* loopGenerator(max: number) {
    for (let i = 0; i < max; i++) {
        yield i;
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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

#[test]
fn test_attached_comment_erased_with_first_declaration() {
    // Comments directly attached (no blank line) to an erased first
    // declaration should be erased with it.
    let source = r#"// Ambient variable
declare var n;

var x = 1;"#;

    let (parser, root) = parse_test_source(source);

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

    // Comment attached to erased declaration should be erased
    assert!(
        !output.contains("// Ambient variable"),
        "Attached comment should be erased with declaration:\n{output}"
    );
    assert!(
        output.contains("var x = 1;"),
        "Code after erased declarations should be emitted:\n{output}"
    );
}

#[test]
fn test_commonjs_detached_comment_before_esmodule_marker() {
    // tsc preserves "detached" comments (followed by blank line) BEFORE
    // the __esModule marker. "Attached" comments (no blank line) are
    // deferred to AFTER the marker.
    let source = r#"/*
 * Public API sample
 */

import ts = require("typescript");

const x = ts.version;"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.take_output();

    // Detached block comment should come BEFORE __esModule
    let comment_pos = output
        .find("Public API sample")
        .expect("comment should be present");
    let esmod_pos = output
        .find("__esModule")
        .expect("__esModule should be present");
    assert!(
        comment_pos < esmod_pos,
        "Detached comment should appear before __esModule marker:\n{output}"
    );
}

/// TODO: Comment stripping in CJS mode strips attached block comments entirely.
/// tsc should emit __esModule BEFORE the attached comment, but currently the comment
/// is stripped from the output. When comment preservation in CJS mode is fixed,
/// update to verify ordering of __esModule and the comment.
#[test]
fn test_commonjs_attached_comment_after_esmodule_marker() {
    let source = r#"/*****************************
* (c) Copyright - Important
****************************/
import model = require("./greeter");
var x = 1;"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.take_output();

    // Currently the comment is stripped entirely in CJS mode.
    // Document current behavior: __esModule should be present, comment may be absent.
    let has_esmod = output.contains("__esModule");
    let has_comment = output.contains("Copyright");
    assert!(
        has_esmod,
        "__esModule marker should be present in CJS output:\n{output}"
    );
    if has_comment {
        // If comment is preserved in the future, verify ordering
        let comment_pos = output.find("Copyright").unwrap();
        let esmod_pos = output.find("__esModule").unwrap();
        assert!(
            esmod_pos < comment_pos,
            "__esModule marker should appear before attached comment:\n{output}"
        );
    }
    // TODO: When comment preservation is fixed, assert has_comment == true
}

#[test]
fn test_computed_property_name_bracket_source_mapping() {
    // tsc maps both `[` and `]` brackets of computed property names to their
    // source positions. Verify that the closing `]` gets a mapping.
    let source = r#"class C {
    ["hello"]() {
        debugger;
    }
}"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");

    let decoded = decode_mappings(mappings_str);

    // Find the source position of `]` in the computed property name `["hello"]`
    // The `]` follows `"hello"` on the same line
    let (bracket_line, bracket_col) = find_line_col(source, "]()");

    let has_closing_bracket_mapping = decoded
        .iter()
        .any(|m| m.original_line == bracket_line && m.original_column == bracket_col);

    assert!(
        has_closing_bracket_mapping,
        "Expected a source map mapping for the closing `]` at source line {bracket_line} col {bracket_col}\n\
         Decoded mappings: {decoded:#?}\nOutput:\n{output}"
    );
}

#[test]
fn test_computed_property_name_getter_bracket_mapping() {
    // Verify that getter with computed property name also maps brackets.
    let source = r#"class C {
    get ["goodbye"]() {
        return 0;
    }
}"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");

    let decoded = decode_mappings(mappings_str);

    // Find the closing `]` in `["goodbye"]`
    let (bracket_line, bracket_col) = find_line_col(source, "]()");

    let has_closing_bracket_mapping = decoded
        .iter()
        .any(|m| m.original_line == bracket_line && m.original_column == bracket_col);

    assert!(
        has_closing_bracket_mapping,
        "Expected a source map mapping for the closing `]` at source line {bracket_line} col {bracket_col}\n\
         Decoded mappings: {decoded:#?}\nOutput:\n{output}"
    );
}

#[test]
fn test_computed_property_object_literal_bracket_mapping() {
    // Verify that object literal computed properties also map brackets.
    let source = r#"var v = {
    ["hello"]() {
        return 0;
    }
};"#;

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");

    let decoded = decode_mappings(mappings_str);

    // Find the closing `]` in `["hello"]`
    let (bracket_line, bracket_col) = find_line_col(source, "]()");

    let has_closing_bracket_mapping = decoded
        .iter()
        .any(|m| m.original_line == bracket_line && m.original_column == bracket_col);

    assert!(
        has_closing_bracket_mapping,
        "Expected a source map mapping for closing `]` at source line {bracket_line} col {bracket_col}\n\
         Decoded mappings: {decoded:#?}\nOutput:\n{output}"
    );
}

/// Verify semicolon mappings for debugger and return statements.
#[test]
fn test_sourcemap_semicolon_mapping() {
    let source = "function f() {\n    debugger;\n    return 42;\n}";
    // Line 0: function f() {
    // Line 1:     debugger;       (`;` at col 12)
    // Line 2:     return 42;      (`;` at col 13)
    // Line 3: }

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let decoded = decode_mappings(mappings_str);

    // Check that `;` after `debugger` is mapped
    // debugger is on source line 1, `;` at col 12
    let has_debugger_semi = decoded
        .iter()
        .any(|m| m.original_line == 1 && m.original_column == 12);
    assert!(
        has_debugger_semi,
        "Expected mapping for `;` after `debugger` at src(1:12)"
    );

    // Check that `;` after `return 42` is mapped
    // return 42 is on source line 2, `;` at col 13
    let has_return_semi = decoded
        .iter()
        .any(|m| m.original_line == 2 && m.original_column == 13);
    assert!(
        has_return_semi,
        "Expected mapping for `;` after `return 42` at src(2:13)"
    );
}

/// Verify source map mappings for try/catch/finally blocks.
#[test]
fn test_sourcemap_try_catch_finally() {
    let source = "try {\n    throw new Error();\n} catch (e) {\n    console.log(e);\n} finally {\n    cleanup();\n}";
    // Line 0: try {
    // Line 1:     throw new Error();
    // Line 2: } catch (e) {
    // Line 3:     console.log(e);
    // Line 4: } finally {
    // Line 5:     cleanup();
    // Line 6: }

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let decoded = decode_mappings(mappings_str);

    // Verify key mappings exist
    // `try` keyword at src(0:0)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 0 && m.original_column == 0),
        "Expected mapping for `try` keyword at src(0:0)"
    );
    // `throw` keyword at src(1:4)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 1 && m.original_column == 4),
        "Expected mapping for `throw` keyword at src(1:4)"
    );
    // `catch` somewhere on line 2
    assert!(
        decoded.iter().any(|m| m.original_line == 2),
        "Expected mapping on catch line (src line 2)"
    );
    // `console` at src(3:4)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 3 && m.original_column == 4),
        "Expected mapping for `console.log(e)` at src(3:4)"
    );
    // `finally` somewhere on line 4
    assert!(
        decoded.iter().any(|m| m.original_line == 4),
        "Expected mapping on finally line (src line 4)"
    );
    // `cleanup` at src(5:4)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 5 && m.original_column == 4),
        "Expected mapping for `cleanup()` at src(5:4)"
    );
    // closing `}` on line 6
    assert!(
        decoded.iter().any(|m| m.original_line == 6),
        "Expected mapping on closing brace line (src line 6)"
    );
    // Semicolons: `;` after `throw new Error()` at src(1:21)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 1 && m.original_column == 21),
        "Expected mapping for `;` after `throw new Error()` at src(1:21)"
    );

    assert!(output.contains("try {"), "Output should contain 'try {{'");
    assert!(
        output.contains("catch (e)"),
        "Output should contain 'catch (e)'"
    );
}

/// Verify source map mappings for for-loop and if/else statements.
#[test]
fn test_sourcemap_for_loop_and_if_else() {
    let source = "for (let i = 0; i < 10; i++) {\n    if (i > 5) {\n        break;\n    } else {\n        continue;\n    }\n}";
    // Line 0: for (let i = 0; i < 10; i++) {
    // Line 1:     if (i > 5) {
    // Line 2:         break;
    // Line 3:     } else {
    // Line 4:         continue;
    // Line 5:     }
    // Line 6: }

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let decoded = decode_mappings(mappings_str);

    // `for` keyword at src(0:0)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 0 && m.original_column == 0),
        "Expected mapping for `for` keyword at src(0:0)"
    );
    // `if` keyword at src(1:4)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 1 && m.original_column == 4),
        "Expected mapping for `if` keyword at src(1:4)"
    );
    // `break` at src(2:8)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 2 && m.original_column == 8),
        "Expected mapping for `break` at src(2:8)"
    );
    // `break;` semicolon at src(2:13)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 2 && m.original_column == 13),
        "Expected mapping for `;` after `break` at src(2:13)"
    );
    // `continue` at src(4:8)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 4 && m.original_column == 8),
        "Expected mapping for `continue` at src(4:8)"
    );
    // `continue;` semicolon at src(4:16)
    assert!(
        decoded
            .iter()
            .any(|m| m.original_line == 4 && m.original_column == 16),
        "Expected mapping for `;` after `continue` at src(4:16)"
    );

    assert!(
        output.contains("for (let i = 0;"),
        "Output should contain for loop"
    );
}

/// Compare tsz source map output against tsc's baseline for switch statements.
#[test]
fn test_sourcemap_parity_switch() {
    let source = "var x = 10;\n\
                   switch (x) {\n\
                   \x20\x20\x20\x20case 5:\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20x++;\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20break;\n\
                   \x20\x20\x20\x20case 10:\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20{\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20x--;\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break;\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20}\n\
                   \x20\x20\x20\x20default:\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20x = x *10;\n\
                   }";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from sourceMapValidationSwitch.js.map, first switch only)
    // tsc emits "use strict"; on gen line 0, so tsc gen lines are 1-indexed relative to ours.
    // tsc mappings for first switch block (gen lines 1-13):
    let tsc_mappings = ";AAAA,IAAI,CAAC,GAAG,EAAE,CAAC;AACX,QAAQ,CAAC,EAAE,CAAC;IACR,KAAK,CAAC;QACF,CAAC,EAAE,CAAC;QACJ,MAAM;IACV,KAAK,EAAE;QACH,CAAC;YACG,CAAC,EAAE,CAAC;YACJ,MAAM;QACV,CAAC;IACL;QACI,CAAC,GAAG,CAAC,GAAE,EAAE,CAAC;AAClB,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    // Track parity progress (switch)
    const EXPECTED_MISSING: usize = 0;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        msg.push_str(&format!("\nOutput:\n{output}"));
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

/// Compare tsz source map output against tsc's baseline for while loops.
#[test]
fn test_sourcemap_parity_while() {
    let source = "var a = 10;\n\
                   while (a == 10) {\n\
                   \x20\x20\x20\x20a++;\n\
                   }";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings for first while loop (gen lines 1-4)
    let tsc_mappings = ";AAAA,IAAI,CAAC,GAAG,EAAE,CAAC;AACX,OAAO,CAAC,IAAI,EAAE,EAAE,CAAC;IACb,CAAC,EAAE,CAAC;AACR,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 0;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        msg.push_str(&format!("\nOutput:\n{output}"));
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

/// Compare tsz source map output against tsc's baseline for do-while loops.
#[test]
fn test_sourcemap_parity_do_while() {
    let source = "var i = 0;\n\
                   do\n\
                   {\n\
                   \x20\x20\x20\x20i++;\n\
                   } while (i < 10);\n\
                   do {\n\
                   \x20\x20\x20\x20i++;\n\
                   } while (i < 20);";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings for do-while
    let tsc_mappings = ";AAAA,IAAI,CAAC,GAAG,CAAC,CAAC;AACV,GACA,CAAC;IACG,CAAC,EAAE,CAAC;AACR,CAAC,QAAQ,CAAC,GAAG,EAAE,EAAE;AACjB,GAAG,CAAC;IACA,CAAC,EAAE,CAAC;AACR,CAAC,QAAQ,CAAC,GAAG,EAAE,EAAE";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 0;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        msg.push_str(&format!("\nOutput:\n{output}"));
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

/// Compare tsz source map output against tsc's baseline for if/else.
#[test]
fn test_sourcemap_parity_if_else() {
    // Source from sourceMapValidationIfElse.ts
    let source = "var i = 10;\n\
                   if (i == 10) {\n\
                   \x20\x20\x20\x20i++;\n\
                   } else\n\
                   {\n\
                   }\n\
                   if (i == 10)\n\
                   {\n\
                   \x20\x20\x20\x20i++;\n\
                   }\n\
                   else if (i == 20) {\n\
                   \x20\x20\x20\x20i--;\n\
                   } else if (i == 30) {\n\
                   \x20\x20\x20\x20i += 70;\n\
                   } else {\n\
                   \x20\x20\x20\x20i--;\n\
                   }";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from sourceMapValidationIfElse.js.map)
    let tsc_mappings = ";AAAA,IAAI,CAAC,GAAG,EAAE,CAAC;AACX,IAAI,CAAC,IAAI,EAAE,EAAE,CAAC;IACV,CAAC,EAAE,CAAC;AACR,CAAC;KACD,CAAC;AACD,CAAC;AACD,IAAI,CAAC,IAAI,EAAE,EACX,CAAC;IACG,CAAC,EAAE,CAAC;AACR,CAAC;KACI,IAAI,CAAC,IAAI,EAAE,EAAE,CAAC;IACf,CAAC,EAAE,CAAC;AACR,CAAC;KAAM,IAAI,CAAC,IAAI,EAAE,EAAE,CAAC;IACjB,CAAC,IAAI,EAAE,CAAC;AACZ,CAAC;KAAM,CAAC;IACJ,CAAC,EAAE,CAAC;AACR,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 0;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

/// Compare tsz source map output against tsc's baseline for try/catch/finally.
#[test]
fn test_sourcemap_parity_try_catch_finally() {
    // Source from sourceMapValidationTryCatchFinally.ts
    let source = "var x = 10;\n\
                   try {\n\
                   \x20\x20\x20\x20x = x + 1;\n\
                   } catch (e) {\n\
                   \x20\x20\x20\x20x = x - 1;\n\
                   } finally {\n\
                   \x20\x20\x20\x20x = x * 10;\n\
                   }\n\
                   try\n\
                   {\n\
                   \x20\x20\x20\x20x = x + 1;\n\
                   \x20\x20\x20\x20throw new Error();\n\
                   }\n\
                   catch (e)\n\
                   {\n\
                   \x20\x20\x20\x20x = x - 1;\n\
                   }\n\
                   finally\n\
                   {\n\
                   \x20\x20\x20\x20x = x * 10;\n\
                   }";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from sourceMapValidationTryCatchFinally.js.map)
    let tsc_mappings = ";AAAA,IAAI,CAAC,GAAG,EAAE,CAAC;AACX,IAAI,CAAC;IACD,CAAC,GAAG,CAAC,GAAG,CAAC,CAAC;AACd,CAAC;AAAC,OAAO,CAAC,EAAE,CAAC;IACT,CAAC,GAAG,CAAC,GAAG,CAAC,CAAC;AACd,CAAC;QAAS,CAAC;IACP,CAAC,GAAG,CAAC,GAAG,EAAE,CAAC;AACf,CAAC;AACD,IACA,CAAC;IACG,CAAC,GAAG,CAAC,GAAG,CAAC,CAAC;IACV,MAAM,IAAI,KAAK,EAAE,CAAC;AACtB,CAAC;AACD,OAAO,CAAC,EACR,CAAC;IACG,CAAC,GAAG,CAAC,GAAG,CAAC,CAAC;AACd,CAAC;QAED,CAAC;IACG,CAAC,GAAG,CAAC,GAAG,EAAE,CAAC;AACf,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 2;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

/// Compare tsz source map output against tsc's baseline for the
/// `computedPropertyNamesSourceMap1_ES6` conformance test.
#[test]
fn test_sourcemap_parity_computed_property_names_es6() {
    // Source from the conformance test (note: uses tabs for indentation)
    let source = "class C {\n\
                       \x20\x20\x20\x20[\"hello\"]() {\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20debugger;\n\
                   \t}\n\
                   \tget [\"goodbye\"]() {\n\
                   \t\treturn 0;\n\
                   \t}\n\
                   }";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from computedPropertyNamesSourceMap1_ES6.js.map)
    let tsc_mappings = ";AAAA,MAAM,CAAC;IACH,CAAC,OAAO,CAAC;QACL,QAAQ,CAAC;IAChB,CAAC;IACD,IAAI,CAAC,SAAS,CAAC;QACd,OAAO,CAAC,CAAC;IACV,CAAC;CACD";
    let tsc_decoded = decode_mappings(tsc_mappings);

    // tsc emits "use strict"; on line 0, shifting generated lines by 1.
    // Adjust tsc mappings by subtracting 1 from generated_line for comparison.
    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    // Track parity progress: fail if we regress (more missing than expected).
    // Update EXPECTED_MISSING as we fix more mappings.
    const EXPECTED_MISSING: usize = 4;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        msg.push_str(&format!("\nOutput:\n{output}"));
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

/// Compare tsz source map output against tsc's baseline for for-in statements.
#[test]
fn test_sourcemap_parity_for_in() {
    // Source from sourceMapValidationForIn.ts (without directives)
    let source = "for (var x in String) {\n\
                   \x20\x20\x20\x20WScript.Echo(x);\n\
                   }\n\
                   for (x in String) {\n\
                   \x20\x20\x20\x20WScript.Echo(x);\n\
                   }\n\
                   for (var x2 in String)\n\
                   {\n\
                   \x20\x20\x20\x20WScript.Echo(x2);\n\
                   }\n\
                   for (x in String)\n\
                   {\n\
                   \x20\x20\x20\x20WScript.Echo(x);\n\
                   }";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from sourceMapValidationForIn.js.map)
    let tsc_mappings = ";AAAA,KAAK,IAAI,CAAC,IAAI,MAAM,EAAE,CAAC;IACnB,OAAO,CAAC,IAAI,CAAC,CAAC,CAAC,CAAC;AACpB,CAAC;AACD,KAAK,CAAC,IAAI,MAAM,EAAE,CAAC;IACf,OAAO,CAAC,IAAI,CAAC,CAAC,CAAC,CAAC;AACpB,CAAC;AACD,KAAK,IAAI,EAAE,IAAI,MAAM,EACrB,CAAC;IACG,OAAO,CAAC,IAAI,CAAC,EAAE,CAAC,CAAC;AACrB,CAAC;AACD,KAAK,CAAC,IAAI,MAAM,EAChB,CAAC;IACG,OAAO,CAAC,IAAI,CAAC,CAAC,CAAC,CAAC;AACpB,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 8;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

/// Compare tsz source map output against tsc's baseline for function declarations.
#[test]
fn test_sourcemap_parity_functions() {
    // Source from sourceMapValidationFunctions.ts (without directives)
    let source = "var greetings = 0;\n\
                   function greet(greeting: string): number {\n\
                   \x20\x20\x20\x20greetings++;\n\
                   \x20\x20\x20\x20return greetings;\n\
                   }\n\
                   function greet2(greeting: string, n = 10, x?: string, ...restParams: string[]): number {\n\
                   \x20\x20\x20\x20greetings++;\n\
                   \x20\x20\x20\x20return greetings;\n\
                   }\n\
                   function foo(greeting: string, n = 10, x?: string, ...restParams: string[])\n\
                   {\n\
                   \x20\x20\x20\x20return;\n\
                   }";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from sourceMapValidationFunctions.js.map)
    let tsc_mappings = ";AAAA,IAAI,SAAS,GAAG,CAAC,CAAC;AAClB,SAAS,KAAK,CAAC,QAAgB;IAC3B,SAAS,EAAE,CAAC;IACZ,OAAO,SAAS,CAAC;AACrB,CAAC;AACD,SAAS,MAAM,CAAC,QAAgB,EAAE,CAAC,GAAG,EAAE,EAAE,CAAU,EAAE,GAAG,UAAoB;IACzE,SAAS,EAAE,CAAC;IACZ,OAAO,SAAS,CAAC;AACrB,CAAC;AACD,SAAS,GAAG,CAAC,QAAgB,EAAE,CAAC,GAAG,EAAE,EAAE,CAAU,EAAE,GAAG,UAAoB;IAEtE,OAAO;AACX,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 10;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

#[test]
fn test_sourcemap_parity_statements() {
    // Source from sourceMapValidationStatements.ts (without directives)
    let source = "function f() {\n    var y;\n    var x = 0;\n    for (var i = 0; i < 10; i++) {\n        x += i;\n        x *= 0;\n    }\n    if (x > 17) {\n        x /= 9;\n    } else {\n        x += 10;\n        x++;\n    }\n    var a = [\n        1,\n        2,\n        3\n    ];\n    var obj = {\n        z: 1,\n        q: \"hello\"\n    };\n    for (var j in a) {\n        obj.z = a[j];\n        var v = 10;\n    }\n    try {\n        obj.q = \"ohhh\";\n    } catch (e) {\n        if (obj.z < 10) {\n            obj.z = 12;\n        } else {\n            obj.q = \"hmm\";\n        }\n    }\n    try {\n        throw new Error();\n    } catch (e1) {\n        var b = e1;\n    } finally {\n        y = 70;\n    }\n    with (obj) {\n        i = 2;\n        z = 10;\n    }\n    switch (obj.z) {\n        case 0: {\n            x++;\n            break;\n\n        }\n        case 1: {\n            x--;\n            break;\n\n        }\n        default: {\n            x *= 2;\n            x = 50;\n            break;\n\n        }\n    }\n    while (x < 10) {\n        x++;\n    }\n    do {\n        x--;\n    } while (x > 4)\n    x = y;\n    var z = (x == 1) ? x + 1 : x - 1;\n    (x == 1) ? x + 1 : x - 1;\n    x === 1;\n    x = z = 40;\n    eval(\"y\");\n    return;\n}\nvar b = function () {\n    var x = 10;\n    x = x + 1;\n};\nf();";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from sourceMapValidationStatements.js.map)
    // tsc gen line 0 = "use strict"; so tsc gen lines are offset +1 from tsz
    let tsc_mappings = ";AAAA,SAAS,CAAC;IACN,IAAI,CAAC,CAAC;IACN,IAAI,CAAC,GAAG,CAAC,CAAC;IACV,KAAK,IAAI,CAAC,GAAG,CAAC,EAAE,CAAC,GAAG,EAAE,EAAE,CAAC,EAAE,EAAE,CAAC;QAC1B,CAAC,IAAI,CAAC,CAAC;QACP,CAAC,IAAI,CAAC,CAAC;IACX,CAAC;IACD,IAAI,CAAC,GAAG,EAAE,EAAE,CAAC;QACT,CAAC,IAAI,CAAC,CAAC;IACX,CAAC;SAAM,CAAC;QACJ,CAAC,IAAI,EAAE,CAAC;QACR,CAAC,EAAE,CAAC;IACR,CAAC;IACD,IAAI,CAAC,GAAG;QACJ,CAAC;QACD,CAAC;QACD,CAAC;KACJ,CAAC;IACF,IAAI,GAAG,GAAG;QACN,CAAC,EAAE,CAAC;QACJ,CAAC,EAAE,OAAO;KACb,CAAC;IACF,KAAK,IAAI,CAAC,IAAI,CAAC,EAAE,CAAC;QACd,GAAG,CAAC,CAAC,GAAG,CAAC,CAAC,CAAC,CAAC,CAAC;QACb,IAAI,CAAC,GAAG,EAAE,CAAC;IACf,CAAC;IACD,IAAI,CAAC;QACD,GAAG,CAAC,CAAC,GAAG,MAAM,CAAC;IACnB,CAAC;IAAC,OAAO,CAAC,EAAE,CAAC;QACT,IAAI,GAAG,CAAC,CAAC,GAAG,EAAE,EAAE,CAAC;YACb,GAAG,CAAC,CAAC,GAAG,EAAE,CAAC;QACf,CAAC;aAAM,CAAC;YACJ,GAAG,CAAC,CAAC,GAAG,KAAK,CAAC;QAClB,CAAC;IACL,CAAC;IACD,IAAI,CAAC;QACD,MAAM,IAAI,KAAK,EAAE,CAAC;IACtB,CAAC;IAAC,OAAO,EAAE,EAAE,CAAC;QACV,IAAI,CAAC,GAAG,EAAE,CAAC;IACf,CAAC;YAAS,CAAC;QACP,CAAC,GAAG,EAAE,CAAC;IACX,CAAC;IACD,MAAM,GAAG,EAAE,CAAC;QACR,CAAC,GAAG,CAAC,CAAC;QACN,CAAC,GAAG,EAAE,CAAC;IACX,CAAC;IACD,QAAQ,GAAG,CAAC,CAAC,EAAE,CAAC;QACZ,KAAK,CAAC,CAAC,CAAC,CAAC;YACL,CAAC,EAAE,CAAC;YACJ,MAAM;QAEV,CAAC;QACD,KAAK,CAAC,CAAC,CAAC,CAAC;YACL,CAAC,EAAE,CAAC;YACJ,MAAM;QAEV,CAAC;QACD,OAAO,CAAC,CAAC,CAAC;YACN,CAAC,IAAI,CAAC,CAAC;YACP,CAAC,GAAG,EAAE,CAAC;YACP,MAAM;QAEV,CAAC;IACL,CAAC;IACD,OAAO,CAAC,GAAG,EAAE,EAAE,CAAC;QACZ,CAAC,EAAE,CAAC;IACR,CAAC;IACD,GAAG,CAAC;QACA,CAAC,EAAE,CAAC;IACR,CAAC,QAAQ,CAAC,GAAG,CAAC,EAAC;IACf,CAAC,GAAG,CAAC,CAAC;IACN,IAAI,CAAC,GAAG,CAAC,CAAC,IAAI,CAAC,CAAC,CAAC,CAAC,CAAC,CAAC,GAAG,CAAC,CAAC,CAAC,CAAC,CAAC,GAAG,CAAC,CAAC;IACjC,CAAC,CAAC,IAAI,CAAC,CAAC,CAAC,CAAC,CAAC,CAAC,GAAG,CAAC,CAAC,CAAC,CAAC,CAAC,GAAG,CAAC,CAAC;IACzB,CAAC,KAAK,CAAC,CAAC;IACR,CAAC,GAAG,CAAC,GAAG,EAAE,CAAC;IACX,IAAI,CAAC,GAAG,CAAC,CAAC;IACV,OAAO;AACX,CAAC;AACD,IAAI,CAAC,GAAG;IACJ,IAAI,CAAC,GAAG,EAAE,CAAC;IACX,CAAC,GAAG,CAAC,GAAG,CAAC,CAAC;AACd,CAAC,CAAC;AACF,CAAC,EAAE,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 321;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

#[test]
fn test_sourcemap_parity_lambda_multiline() {
    // Source from sourceMapValidationLambdaSpanningMultipleLines.ts
    // @target: es2015
    let source = "((item: string) =>\n    item\n)";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from sourceMapValidationLambdaSpanningMultipleLines.js.map)
    let tsc_mappings = ";AAAA,CAAC,CAAC,IAAY,EAAE,EAAE,CACd,IAAI,CACP,CAAA";
    let tsc_decoded = decode_mappings(tsc_mappings);

    // tsc emits "use strict"; on line 0, shifting generated lines by 1.
    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 4;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        msg.push_str(&format!("\nOutput:\n{output}"));
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

#[test]
fn test_sourcemap_parity_class_extends() {
    // Source from sourceMapValidationClassWithDefaultConstructorAndExtendsClause.ts
    // @target: es2015
    let source = "class AbstractGreeter {\n\
                   }\n\
                   \n\
                   class Greeter extends AbstractGreeter {\n\
                   \x20\x20\x20\x20public a = 10;\n\
                   \x20\x20\x20\x20public nameA = \"Ten\";\n\
                   }";

    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        ..PrinterOptions::default()
    };

    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (from sourceMapValidationClassWithDefaultConstructorAndExtendsClause.js.map)
    let tsc_mappings = ";AAAA,MAAM,eAAe;CACpB;AAED,MAAM,OAAQ,SAAQ,eAAe;IAArC;;QACW,MAAC,GAAG,EAAE,CAAC;QACP,UAAK,GAAG,KAAK,CAAC;IACzB,CAAC;CAAA";
    let tsc_decoded = decode_mappings(tsc_mappings);

    // tsc emits "use strict"; on line 0, shifting generated lines by 1.
    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line.saturating_sub(1);
        let found = tsz_decoded.iter().any(|tsz_m| {
            tsz_m.generated_line == adjusted_gen_line
                && tsz_m.generated_column == tsc_m.generated_column
                && tsz_m.original_line == tsc_m.original_line
                && tsz_m.original_column == tsc_m.original_column
        });
        if !found {
            missing.push((tsc_m, adjusted_gen_line));
        }
    }

    const EXPECTED_MISSING: usize = 16;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{}) [tsz missing]\n",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            ));
        }
        msg.push_str(&format!("\ntsz mappings ({}):\n", tsz_decoded.len()));
        for m in &tsz_decoded {
            msg.push_str(&format!(
                "  gen({}:{}) -> src({}:{})\n",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            ));
        }
        msg.push_str(&format!("\nOutput:\n{output}"));
        panic!("{msg}");
    }
    if num_missing.cmp(&EXPECTED_MISSING) == std::cmp::Ordering::Less {
        panic!(
            "IMPROVEMENT: only {num_missing} tsc mappings missing (was {EXPECTED_MISSING}). \
             Update EXPECTED_MISSING to {num_missing}."
        );
    }
}

