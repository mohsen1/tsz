#[test]
fn test_source_map_generator_es5_control_flow() {
    let source = r#"function* controlFlowGenerator(n: number): Generator<number> {
    for (let i = 0; i < n; i++) {
        if (i % 2 === 0) {
            yield i * 2;
        } else {
            yield i * 3;
        }
    }

    let j = 0;
    while (j < 3) {
        yield j * 10;
        j++;
    }

    switch (n) {
        case 1: yield 100; break;
        case 2: yield 200; break;
        default: yield 999;
    }
}

const gen = controlFlowGenerator(5);
console.log([...gen]);"#;

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
        output.contains("controlFlowGenerator"),
        "expected output to contain controlFlowGenerator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for control flow generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_state_machine() {
    let source = r#"type State = 'idle' | 'loading' | 'success' | 'error';

function* stateMachine(): Generator<State, void, string> {
    let input: string;

    while (true) {
        yield 'idle';
        input = yield 'loading';

        if (input === 'success') {
            yield 'success';
        } else if (input === 'error') {
            yield 'error';
        }
    }
}

const machine = stateMachine();
console.log(machine.next());
console.log(machine.next());
console.log(machine.next('success'));"#;

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
        output.contains("stateMachine"),
        "expected output to contain stateMachine. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for state machine generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_finally() {
    let source = r#"function* generatorWithFinally(): Generator<number> {
    try {
        yield 1;
        yield 2;
        yield 3;
    } finally {
        console.log('Generator cleanup');
    }
}

function* nestedTryFinally(): Generator<string> {
    try {
        try {
            yield 'inner-1';
            yield 'inner-2';
        } finally {
            yield 'inner-finally';
        }
        yield 'outer-1';
    } finally {
        yield 'outer-finally';
    }
}

const gen1 = generatorWithFinally();
const gen2 = nestedTryFinally();
console.log([...gen1], [...gen2]);"#;

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
        output.contains("generatorWithFinally") || output.contains("nestedTryFinally"),
        "expected output to contain generator functions. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for finally generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_composition() {
    let source = r#"function* numbers(n: number): Generator<number> {
    for (let i = 1; i <= n; i++) {
        yield i;
    }
}

function* letters(s: string): Generator<string> {
    for (const c of s) {
        yield c;
    }
}

function* combined(): Generator<number | string> {
    yield* numbers(3);
    yield '---';
    yield* letters('abc');
    yield '---';
    yield* numbers(2);
}

function* flatten<T>(iterables: Iterable<T>[]): Generator<T> {
    for (const iterable of iterables) {
        yield* iterable;
    }
}

console.log([...combined()]);
console.log([...flatten([[1, 2], [3, 4], [5]])]);"#;

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
        output.contains("combined") || output.contains("flatten"),
        "expected output to contain composition generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for composition generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
