#[test]
fn test_source_map_generator_es5_infinite() {
    // Test infinite generator patterns
    let source = r#"function* infiniteCounter(): Generator<number, never, unknown> {
    let count = 0;
    while (true) {
        yield count++;
    }
}

function* fibonacci(): Generator<number, never, unknown> {
    let a = 0;
    let b = 1;
    while (true) {
        yield a;
        const temp = a;
        a = b;
        b = temp + b;
    }
}

function* idGenerator(prefix: string): Generator<string, never, unknown> {
    let id = 0;
    while (true) {
        yield prefix + "-" + (id++);
    }
}

const counter = infiniteCounter();
console.log(counter.next().value);
console.log(counter.next().value);

const fib = fibonacci();
for (let i = 0; i < 10; i++) {
    console.log(fib.next().value);
}

const ids = idGenerator("user");
console.log(ids.next().value);"#;

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
        output.contains("infiniteCounter") && output.contains("fibonacci"),
        "expected generator functions in output. output: {output}"
    );
    assert!(
        output.contains("idGenerator"),
        "expected idGenerator function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for infinite generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_class_iterator() {
    // Test generator implementing iterator protocol
    let source = r#"class Range {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator](): Generator<number, void, unknown> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }
}

class Tree<T> {
    constructor(
        public value: T,
        public left?: Tree<T>,
        public right?: Tree<T>
    ) {}

    *inOrder(): Generator<T, void, unknown> {
        if (this.left) {
            yield* this.left.inOrder();
        }
        yield this.value;
        if (this.right) {
            yield* this.right.inOrder();
        }
    }
}

const range = new Range(1, 5);
for (const num of range) {
    console.log(num);
}

const tree = new Tree(2, new Tree(1), new Tree(3));
for (const val of tree.inOrder()) {
    console.log(val);
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

    assert!(
        output.contains("Range") && output.contains("Tree"),
        "expected Range and Tree classes in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for iterator protocol"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_class_methods() {
    // Test generator methods in classes
    let source = r#"class DataProcessor {
    private data: number[] = [];

    constructor(data: number[]) {
        this.data = data;
    }

    *processAll(): Generator<number, void, unknown> {
        for (const item of this.data) {
            yield this.process(item);
        }
    }

    *processFiltered(predicate: (n: number) => boolean): Generator<number, void, unknown> {
        for (const item of this.data) {
            if (predicate(item)) {
                yield this.process(item);
            }
        }
    }

    private process(item: number): number {
        return item * 2;
    }

    static *range(start: number, end: number): Generator<number, void, unknown> {
        for (let i = start; i <= end; i++) {
            yield i;
        }
    }
}

const processor = new DataProcessor([1, 2, 3, 4, 5]);
for (const result of processor.processAll()) {
    console.log(result);
}

for (const result of processor.processFiltered(function(n) { return n > 2; })) {
    console.log(result);
}

for (const n of DataProcessor.range(1, 10)) {
    console.log(n);
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

    assert!(
        output.contains("DataProcessor"),
        "expected DataProcessor class in output. output: {output}"
    );
    assert!(
        output.contains("processAll") && output.contains("processFiltered"),
        "expected generator methods in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class generator methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
