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
