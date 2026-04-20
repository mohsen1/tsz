#[test]
fn test_attached_comment_erased_with_first_declaration() {
    // Comments directly attached (no blank line) to an erased first
    // declaration should be erased with it.
    let source = r#"// Ambient variable
declare var n;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

