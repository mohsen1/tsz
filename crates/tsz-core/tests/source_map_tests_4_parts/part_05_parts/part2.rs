#[test]
fn test_source_map_private_method_es5_chained_calls() {
    let source = r#"class StringBuilder {
    #value = "";

    #append(str: string): this {
        this.#value += str;
        return this;
    }

    #prepend(str: string): this {
        this.#value = str + this.#value;
        return this;
    }

    #wrap(prefix: string, suffix: string): this {
        this.#value = prefix + this.#value + suffix;
        return this;
    }

    #transform(fn: (s: string) => string): this {
        this.#value = fn(this.#value);
        return this;
    }

    add(str: string): this {
        return this.#append(str);
    }

    addBefore(str: string): this {
        return this.#prepend(str);
    }

    surround(prefix: string, suffix: string): this {
        return this.#wrap(prefix, suffix);
    }

    apply(fn: (s: string) => string): this {
        return this.#transform(fn);
    }

    build(): string {
        return this.#value;
    }
}

const result = new StringBuilder()
    .add("Hello")
    .add(" ")
    .add("World")
    .surround("[", "]")
    .apply(s => s.toUpperCase())
    .build();

console.log(result);"#;

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
        output.contains("StringBuilder"),
        "expected StringBuilder in output. output: {output}"
    );
    assert!(
        output.contains("build"),
        "expected build in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained private method calls"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_method_es5_parameters() {
    let source = r#"class ParameterHandler {
    #processDefaults(a: number = 0, b: string = "default"): string {
        return b + ": " + a;
    }

    #processRest(...items: number[]): number {
        return items.reduce((sum, n) => sum + n, 0);
    }

    #processDestructured({ x, y }: { x: number; y: number }): number {
        return x + y;
    }

    #processArrayDestructured([first, second]: [string, string]): string {
        return first + " and " + second;
    }

    #processGeneric<T>(value: T, transform: (v: T) => string): string {
        return transform(value);
    }

    withDefaults(a?: number, b?: string): string {
        return this.#processDefaults(a, b);
    }

    withRest(...nums: number[]): number {
        return this.#processRest(...nums);
    }

    withObject(obj: { x: number; y: number }): number {
        return this.#processDestructured(obj);
    }

    withArray(arr: [string, string]): string {
        return this.#processArrayDestructured(arr);
    }

    withGeneric<T>(val: T, fn: (v: T) => string): string {
        return this.#processGeneric(val, fn);
    }
}

const handler = new ParameterHandler();
console.log(handler.withDefaults());
console.log(handler.withDefaults(42, "custom"));
console.log(handler.withRest(1, 2, 3, 4, 5));
console.log(handler.withObject({ x: 10, y: 20 }));
console.log(handler.withArray(["hello", "world"]));
console.log(handler.withGeneric(123, n => n.toString()));"#;

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
        output.contains("ParameterHandler"),
        "expected ParameterHandler in output. output: {output}"
    );
    assert!(
        output.contains("withDefaults"),
        "expected withDefaults in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private methods with parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
