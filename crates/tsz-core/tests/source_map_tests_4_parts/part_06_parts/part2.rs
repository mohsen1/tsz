/// Test optional chaining with chained methods with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_chained_methods() {
    let source = r#"interface Builder {
    setName?(name: string): Builder;
    setValue?(value: number): Builder;
    build?(): object;
}

function buildObject(builder: Builder | null) {
    return builder?.setName?.("test")?.setValue?.(42)?.build?.();
}

class FluentApi {
    private data: any;

    with?(key: string): FluentApi | undefined {
        return this;
    }

    get?(): any {
        return this.data;
    }
}

const api = new FluentApi();
const result = api?.with?.("key")?.get?.();
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
        output.contains("buildObject"),
        "expected buildObject in output. output: {output}"
    );
    assert!(
        output.contains("FluentApi"),
        "expected FluentApi in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with delete operator with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_delete() {
    let source = r#"interface Obj {
    prop?: {
        nested?: string;
    };
    items?: string[];
}

function deleteProp(obj: Obj | null) {
    delete obj?.prop?.nested;
}

function deleteElement(obj: Obj | undefined, index: number) {
    delete obj?.items?.[index];
}

const obj: Obj = { prop: { nested: "value" } };
delete obj?.prop?.nested;
delete obj?.items?.[0];
console.log(obj);"#;

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
        output.contains("deleteProp"),
        "expected deleteProp in output. output: {output}"
    );
    assert!(
        output.contains("deleteElement"),
        "expected deleteElement in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for delete operator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test optional chaining with call expression with ES5 target
#[test]
fn test_source_map_optional_chaining_es5_call_expression() {
    let source = r#"type Callback = ((value: number) => void) | undefined;

function invokeCallback(cb: Callback, value: number) {
    cb?.(value);
}

const callbacks: Callback[] = [undefined, (v) => console.log(v)];
callbacks[0]?.(1);
callbacks[1]?.(2);

interface EventEmitter {
    on?: (event: string, handler: Function) => void;
    emit?: (event: string, ...args: any[]) => void;
}

function setupEmitter(emitter: EventEmitter | null) {
    emitter?.on?.("data", console.log);
    emitter?.emit?.("ready");
}

const maybeFunc: (() => number) | null = null;
const result = maybeFunc?.();
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
        output.contains("invokeCallback"),
        "expected invokeCallback in output. output: {output}"
    );
    assert!(
        output.contains("setupEmitter"),
        "expected setupEmitter in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for call expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
