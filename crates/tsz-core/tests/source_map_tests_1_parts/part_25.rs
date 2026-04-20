#[test]
fn test_sourcemap_parity_enums() {
    // Source from sourceMapValidationEnums.ts
    // @target: es2015
    let source = "enum e {\n\
                   \x20\x20\x20\x20x,\n\
                   \x20\x20\x20\x20y,\n\
                   \x20\x20\x20\x20x\n\
                   }\n\
                   enum e2 {\n\
                   \x20\x20\x20\x20x = 10,\n\
                   \x20\x20\x20\x20y = 10,\n\
                   \x20\x20\x20\x20z,\n\
                   \x20\x20\x20\x20x2\n\
                   }\n\
                   enum e3 {\n\
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

    // tsc baseline mappings (from sourceMapValidationEnums.js.map)
    let tsc_mappings = ";AAAA,IAAK,CAIJ;AAJD,WAAK,CAAC;IACF,mBAAC,CAAA;IACD,mBAAC,CAAA;IACD,mBAAC,CAAA;AACL,CAAC,EAJI,CAAC,KAAD,CAAC,QAIL;AACD,IAAK,EAKJ;AALD,WAAK,EAAE;IACH,sBAAM,CAAA;IACN,sBAAM,CAAA;IACN,sBAAC,CAAA;IACD,wBAAE,CAAA;AACN,CAAC,EALI,EAAE,KAAF,EAAE,QAKN;AACD,IAAK,EACJ;AADD,WAAK,EAAE;AACP,CAAC,EADI,EAAE,KAAF,EAAE,QACN";
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

    const EXPECTED_MISSING: usize = 57;
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
fn test_sourcemap_parity_for() {
    // Source from sourceMapValidationFor.ts (@target: es2015)
    let source = "for (var i = 0; i < 10; i++) {\n\
                   \x20\x20\x20\x20WScript.Echo(\"i: \" + i);\n\
                   }\n\
                   for (i = 0; i < 10; i++)\n\
                   {\n\
                   \x20\x20\x20\x20WScript.Echo(\"i: \" + i);\n\
                   }\n\
                   for (var j = 0; j < 10; ) {\n\
                   \x20\x20\x20\x20j++;\n\
                   \x20\x20\x20\x20if (j == 1) {\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20continue;\n\
                   \x20\x20\x20\x20}\n\
                   }\n\
                   for (j = 0; j < 10;)\n\
                   {\n\
                   \x20\x20\x20\x20j++;\n\
                   }\n\
                   for (var k = 0;; k++) {\n\
                   }\n\
                   for (k = 0;; k++)\n\
                   {\n\
                   }\n\
                   for (; k < 10; k++) {\n\
                   }\n\
                   for (;;) {\n\
                   \x20\x20\x20\x20i++;\n\
                   }\n\
                   for (;;)\n\
                   {\n\
                   \x20\x20\x20\x20i++;\n\
                   }\n\
                   for (i = 0, j = 20; j < 20, i < 20; j++) {\n\
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

    // tsc baseline mappings (from sourceMapValidationFor.js.map)
    let tsc_mappings = ";AAAA,KAAK,IAAI,CAAC,GAAG,CAAC,EAAE,CAAC,GAAG,EAAE,EAAE,CAAC,EAAE,EAAE,CAAC;IAC1B,OAAO,CAAC,IAAI,CAAC,KAAK,GAAG,CAAC,CAAC,CAAC;AAC5B,CAAC;AACD,KAAK,CAAC,GAAG,CAAC,EAAE,CAAC,GAAG,EAAE,EAAE,CAAC,EAAE,EACvB,CAAC;IACG,OAAO,CAAC,IAAI,CAAC,KAAK,GAAG,CAAC,CAAC,CAAC;AAC5B,CAAC;AACD,KAAK,IAAI,CAAC,GAAG,CAAC,EAAE,CAAC,GAAG,EAAE,GAAI,CAAC;IACvB,CAAC,EAAE,CAAC;IACJ,IAAI,CAAC,IAAI,CAAC,EAAE,CAAC;QACT,SAAS;IACb,CAAC;AACL,CAAC;AACD,KAAK,CAAC,GAAG,CAAC,EAAE,CAAC,GAAG,EAAE,GAClB,CAAC;IACG,CAAC,EAAE,CAAC;AACR,CAAC;AACD,KAAK,IAAI,CAAC,GAAG,CAAC,GAAG,CAAC,EAAE,EAAE,CAAC;AACvB,CAAC;AACD,KAAK,CAAC,GAAG,CAAC,GAAG,CAAC,EAAE,EAChB,CAAC;AACD,CAAC;AACD,OAAO,CAAC,GAAG,EAAE,EAAE,CAAC,EAAE,EAAE,CAAC;AACrB,CAAC;AACD,SAAS,CAAC;IACN,CAAC,EAAE,CAAC;AACR,CAAC;AACD,SACA,CAAC;IACG,CAAC,EAAE,CAAC;AACR,CAAC;AACD,KAAK,CAAC,GAAG,CAAC,EAAE,CAAC,GAAG,EAAE,EAAE,CAAC,GAAG,EAAE,EAAE,CAAC,GAAG,EAAE,EAAE,CAAC,EAAE,EAAE,CAAC;AAC1C,CAAC";
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

/// Compare tsz source map output against tsc's baseline for return/throw/break/continue.
#[test]
fn test_sourcemap_parity_return_throw_break_continue() {
    let source = "function foo(x) {\n\
                   \x20\x20\x20\x20if (x > 0) {\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20return x;\n\
                   \x20\x20\x20\x20}\n\
                   \x20\x20\x20\x20throw new Error(\"negative\");\n\
                   }\n\
                   function bar() {\n\
                   \x20\x20\x20\x20for (var i = 0; i < 10; i++) {\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20if (i === 5) {\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break;\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20}\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20if (i === 3) {\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20continue;\n\
                   \x20\x20\x20\x20\x20\x20\x20\x20}\n\
                   \x20\x20\x20\x20}\n\
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

    let output = printer.get_output().to_string();
    let map_json = printer
        .generate_source_map_json()
        .expect("source map should be generated");
    let map: Value = serde_json::from_str(&map_json).expect("valid JSON");
    let mappings_str = map["mappings"].as_str().expect("mappings string");
    let tsz_decoded = decode_mappings(mappings_str);

    // tsc baseline mappings (--target es2015 --sourceMap, no type annotations)
    // No "use strict" — gen lines match directly.
    let tsc_mappings = "AAAA,SAAS,GAAG,CAAC,CAAC;IACV,IAAI,CAAC,GAAG,CAAC,EAAE,CAAC;QACR,OAAO,CAAC,CAAC;IACb,CAAC;IACD,MAAM,IAAI,KAAK,CAAC,UAAU,CAAC,CAAC;AAChC,CAAC;AACD,SAAS,GAAG;IACR,KAAK,IAAI,CAAC,GAAG,CAAC,EAAE,CAAC,GAAG,EAAE,EAAE,CAAC,EAAE,EAAE,CAAC;QAC1B,IAAI,CAAC,KAAK,CAAC,EAAE,CAAC;YACV,MAAM;QACV,CAAC;QACD,IAAI,CAAC,KAAK,CAAC,EAAE,CAAC;YACV,SAAS;QACb,CAAC;IACL,CAAC;IACD,OAAO;AACX,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line;
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
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{})\n",
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

/// Compare tsz source map output against tsc's baseline for object/array literals.
#[test]
fn test_sourcemap_parity_object_array_literals() {
    let print_all = true;
    let source = "let obj = {\n\
                       x: 1,\n\
                       y: 2,\n\
                       add() { return this.x + this.y; },\n\
                       name: \"test\"\n\
                   };\n\
                   let arr = [1, 2, 3];\n\
                   let nested = { a: { b: 1 }, c: [4, 5] };";

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

    // tsc baseline mappings (--target es2015 --sourceMap)
    let tsc_mappings = "AAAA,IAAI,GAAG,GAAG;IACN,CAAC,EAAE,CAAC;IACJ,CAAC,EAAE,CAAC;IACJ,GAAG,KAAK,OAAO,IAAI,CAAC,CAAC,GAAG,IAAI,CAAC,CAAC,CAAC,CAAC,CAAC;IACjC,IAAI,EAAE,MAAM;CACf,CAAC;AACF,IAAI,GAAG,GAAG,CAAC,CAAC,EAAE,CAAC,EAAE,CAAC,CAAC,CAAC;AACpB,IAAI,MAAM,GAAG,EAAE,CAAC,EAAE,EAAE,CAAC,EAAE,CAAC,EAAE,EAAE,CAAC,EAAE,CAAC,CAAC,EAAE,CAAC,CAAC,EAAE,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line;
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

    if print_all {
        let num_missing = missing.len();
        println!("Missing {num_missing} mappings:");
        for (m, adj_line) in &missing {
            println!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{})",
                m.generated_line,
                m.generated_column,
                adj_line,
                m.generated_column,
                m.original_line,
                m.original_column
            );
        }
        println!("\ntsz mappings ({}):", tsz_decoded.len());
        for m in &tsz_decoded {
            println!(
                "  gen({}:{}) -> src({}:{})",
                m.generated_line, m.generated_column, m.original_line, m.original_column
            );
        }
        println!("\nOutput:\n{output}");
    }

    const EXPECTED_MISSING: usize = 62;
    let num_missing = missing.len();
    if num_missing > EXPECTED_MISSING {
        let mut msg = format!(
            "REGRESSION: {num_missing} tsc mappings missing (expected at most {EXPECTED_MISSING}):\n",
        );
        for (m, adj_line) in &missing {
            msg.push_str(&format!(
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{})\n",
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

/// Compare tsz source map output against tsc's baseline for variable declarations/assignments.
#[test]
fn test_sourcemap_parity_variables() {
    let source = "var x = 10;\n\
                   var y = 20, z = 30;\n\
                   let a = x + y;\n\
                   const b = a * z;\n\
                   x = x + 1;\n\
                   y += 2;";

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

    // tsc baseline mappings (--target es2015 --sourceMap)
    let tsc_mappings = "AAAA,IAAI,CAAC,GAAG,EAAE,CAAC;AACX,IAAI,CAAC,GAAG,EAAE,EAAE,CAAC,GAAG,EAAE,CAAC;AACnB,IAAI,CAAC,GAAG,CAAC,GAAG,CAAC,CAAC;AACd,MAAM,CAAC,GAAG,CAAC,GAAG,CAAC,CAAC;AAChB,CAAC,GAAG,CAAC,GAAG,CAAC,CAAC;AACV,CAAC,IAAI,CAAC,CAAC";
    let tsc_decoded = decode_mappings(tsc_mappings);

    // No "use strict" — gen lines match directly.
    let mut missing = Vec::new();
    for tsc_m in &tsc_decoded {
        let adjusted_gen_line = tsc_m.generated_line;
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
                "  tsc gen({}:{}) [adj gen({}:{})] -> src({}:{})\n",
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
