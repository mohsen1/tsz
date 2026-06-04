#[test]
fn test_source_map_interface_discriminated_unions() {
    // Test interface with discriminated union patterns
    let source = r#"interface SuccessResult {
    kind: "success";
    data: string;
    timestamp: number;
}

interface ErrorResult {
    kind: "error";
    error: string;
    code: number;
}

interface LoadingResult {
    kind: "loading";
    progress: number;
}

type Result = SuccessResult | ErrorResult | LoadingResult;

interface Action {
    type: string;
}

interface AddAction extends Action {
    type: "add";
    payload: number;
}

interface RemoveAction extends Action {
    type: "remove";
    id: string;
}

type AppAction = AddAction | RemoveAction;

function handleResult(result: Result): string {
    switch (result.kind) {
        case "success":
            return "Data: " + result.data;
        case "error":
            return "Error " + result.code + ": " + result.error;
        case "loading":
            return "Loading: " + result.progress + "%";
    }
}

const success: SuccessResult = { kind: "success", data: "hello", timestamp: Date.now() };
const error: ErrorResult = { kind: "error", error: "Not found", code: 404 };

console.log(handleResult(success));
console.log(handleResult(error));"#;

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
        output.contains("handleResult") || output.contains("success"),
        "expected output to contain handleResult or success. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for discriminated unions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_type_guards() {
    // Test interface with type guard patterns
    let source = r#"interface Fish {
    swim(): void;
    name: string;
}

interface Bird {
    fly(): void;
    name: string;
}

interface Cat {
    meow(): void;
    name: string;
}

type Animal = Fish | Bird | Cat;

function isFish(animal: Animal): animal is Fish {
    return (animal as Fish).swim !== undefined;
}

function isBird(animal: Animal): animal is Bird {
    return (animal as Bird).fly !== undefined;
}

const fish: Fish = {
    name: "Nemo",
    swim() { console.log("Swimming..."); }
};

const bird: Bird = {
    name: "Tweety",
    fly() { console.log("Flying..."); }
};

function handleAnimal(animal: Animal): void {
    if (isFish(animal)) {
        animal.swim();
    } else if (isBird(animal)) {
        animal.fly();
    } else {
        animal.meow();
    }
}

handleAnimal(fish);
handleAnimal(bird);
console.log(fish.name, bird.name);"#;

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
        output.contains("isFish") || output.contains("handleAnimal"),
        "expected output to contain isFish or handleAnimal. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type guards"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_rest_elements() {
    // Test interface with rest elements in types
    let source = r#"interface FunctionWithRest {
    (...args: number[]): number;
}

interface ArrayWithRest {
    items: [string, ...number[]];
    mixed: [boolean, string, ...any[]];
}

interface SpreadParams {
    call(...args: string[]): void;
    apply(first: number, ...rest: string[]): string;
}

const sum: FunctionWithRest = function(...args: number[]): number {
    return args.reduce((a, b) => a + b, 0);
};

const arr: ArrayWithRest = {
    items: ["header", 1, 2, 3, 4],
    mixed: [true, "text", 1, "a", null]
};

const params: SpreadParams = {
    call(...args: string[]): void {
        console.log(args.join(", "));
    },
    apply(first: number, ...rest: string[]): string {
        return first + ": " + rest.join(" ");
    }
};

console.log(sum(1, 2, 3, 4, 5));
console.log(arr.items);
params.call("a", "b", "c");
console.log(params.apply(42, "hello", "world"));"#;

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
        output.contains("sum") || output.contains("params"),
        "expected output to contain sum or params. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest elements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
