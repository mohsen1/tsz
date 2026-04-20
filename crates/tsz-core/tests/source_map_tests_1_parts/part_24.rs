/// Compare tsz source map output against tsc's baseline for while loops.
#[test]
fn test_sourcemap_parity_while() {
    let source = "var a = 10;\n\
                   while (a == 10) {\n\
                   \x20\x20\x20\x20a++;\n\
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

