#[test]
fn test_source_map_interface_index_signature() {
    // Test interface with index signatures
    let source = r#"interface StringDictionary {
    [key: string]: string;
}

interface NumberDictionary {
    [index: number]: string;
    length: number;
}

interface MixedDictionary {
    [key: string]: number | string;
    name: string;
    count: number;
}

const dict: StringDictionary = {
    foo: "bar",
    hello: "world"
};

const numDict: NumberDictionary = {
    0: "first",
    1: "second",
    length: 2
};

const mixed: MixedDictionary = {
    name: "test",
    count: 42,
    extra: "value"
};

function getValues(d: StringDictionary): string[] {
    return Object.values(d);
}

console.log(dict["foo"]);
console.log(numDict[0]);
console.log(mixed.name, mixed.count);
console.log(getValues(dict));"#;

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
        output.contains("dict") && output.contains("getValues"),
        "expected output to contain dict and getValues. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for index signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_call_signature() {
    // Test interface with call signatures
    let source = r#"interface StringProcessor {
    (input: string): string;
}

interface Calculator {
    (a: number, b: number): number;
    description: string;
}

interface Formatter {
    (value: any): string;
    (value: any, format: string): string;
}

const uppercase: StringProcessor = function(input) {
    return input.toUpperCase();
};

const add: Calculator = function(a, b) {
    return a + b;
};
add.description = "Adds two numbers";

const format: Formatter = function(value: any, fmt?: string) {
    if (fmt) {
        return fmt + ": " + String(value);
    }
    return String(value);
};

console.log(uppercase("hello"));
console.log(add(5, 3), add.description);
console.log(format(42));
console.log(format(42, "Number"));"#;

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
        output.contains("uppercase") && output.contains("format"),
        "expected output to contain uppercase and format. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for call signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_construct_signature() {
    // Test interface with construct signatures
    let source = r#"interface PointConstructor {
    new(x: number, y: number): { x: number; y: number };
}

interface ClockConstructor {
    new(hour: number, minute: number): ClockInterface;
}

interface ClockInterface {
    tick(): void;
    getTime(): string;
}

function createPoint(ctor: PointConstructor, x: number, y: number) {
    return new ctor(x, y);
}

const PointClass: PointConstructor = class {
    constructor(public x: number, public y: number) {}
};

const point = createPoint(PointClass, 10, 20);
console.log(point.x, point.y);"#;

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
        output.contains("createPoint") || output.contains("PointClass"),
        "expected output to contain createPoint or PointClass. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for construct signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_merging() {
    // Test interface merging (declaration merging)
    let source = r#"interface Box {
    height: number;
    width: number;
}

interface Box {
    depth: number;
    color: string;
}

interface Box {
    weight?: number;
}

const box: Box = {
    height: 10,
    width: 20,
    depth: 30,
    color: "red"
};

const heavyBox: Box = {
    height: 5,
    width: 5,
    depth: 5,
    color: "blue",
    weight: 100
};

function describeBox(b: Box): string {
    let desc = b.color + " box: " + b.width + "x" + b.height + "x" + b.depth;
    if (b.weight) {
        desc += " (" + b.weight + "kg)";
    }
    return desc;
}

console.log(describeBox(box));
console.log(describeBox(heavyBox));"#;

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
        output.contains("box") && output.contains("describeBox"),
        "expected output to contain box and describeBox. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface merging"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
