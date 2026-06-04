#[test]
fn test_source_map_decorator_composition_es5_chained() {
    let source = r#"function log(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Calling:", key);
        return original.apply(this, args);
    };
    return descriptor;
}

function measure(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        const start = Date.now();
        const result = original.apply(this, args);
        console.log("Duration:", Date.now() - start);
        return result;
    };
    return descriptor;
}

function validate(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        if (args.some(a => a == null)) throw new Error("Invalid args");
        return original.apply(this, args);
    };
    return descriptor;
}

class Service {
    @log
    @measure
    @validate
    process(data: string) {
        return data.toUpperCase();
    }
}

const service = new Service();
console.log(service.process("test"));"#;

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
        output.contains("Service"),
        "expected Service in output. output: {output}"
    );
    assert!(
        output.contains("process"),
        "expected process in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for chained decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_factory() {
    let source = r#"function log(prefix: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            console.log(prefix, key, args);
            return original.apply(this, args);
        };
        return descriptor;
    };
}

function retry(times: number) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = async function(...args: any[]) {
            for (let i = 0; i < times; i++) {
                try {
                    return await original.apply(this, args);
                } catch (e) {
                    if (i === times - 1) throw e;
                }
            }
        };
        return descriptor;
    };
}

function cache(ttl: number) {
    const store = new Map<string, { value: any; expires: number }>();
    return function(target: any, key: string, descriptor: PropertyDescriptor) {
        const original = descriptor.value;
        descriptor.value = function(...args: any[]) {
            const cacheKey = JSON.stringify(args);
            const cached = store.get(cacheKey);
            if (cached && cached.expires > Date.now()) return cached.value;
            const result = original.apply(this, args);
            store.set(cacheKey, { value: result, expires: Date.now() + ttl });
            return result;
        };
        return descriptor;
    };
}

class Api {
    @log("[API]")
    @retry(3)
    @cache(5000)
    async fetchData(id: number) {
        return { id, data: "result" };
    }
}

const api = new Api();
api.fetchData(1).then(console.log);"#;

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
        output.contains("Api"),
        "expected Api in output. output: {output}"
    );
    assert!(
        output.contains("fetchData"),
        "expected fetchData in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for factory decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_composition_es5_metadata() {
    let source = r#"const metadata = new Map<any, Map<string, any>>();

function setMetadata(key: string, value: any) {
    return function(target: any, propertyKey?: string) {
        let targetMeta = metadata.get(target) || new Map();
        targetMeta.set(key, value);
        metadata.set(target, targetMeta);
    };
}

function getMetadata(key: string, target: any): any {
    const targetMeta = metadata.get(target);
    return targetMeta?.get(key);
}

@setMetadata("version", "1.0.0")
@setMetadata("author", "Team")
class Component {
    @setMetadata("required", true)
    name: string = "";

    @setMetadata("type", "handler")
    @setMetadata("async", true)
    async handleEvent(event: any) {
        console.log(event);
    }
}

const comp = new Component();
console.log(getMetadata("version", Component));
console.log(getMetadata("author", Component));"#;

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
        output.contains("Component"),
        "expected Component in output. output: {output}"
    );
    assert!(
        output.contains("metadata"),
        "expected metadata in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for metadata decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
