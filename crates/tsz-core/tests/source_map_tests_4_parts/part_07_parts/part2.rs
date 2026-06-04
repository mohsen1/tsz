/// Test logical assignment in class methods with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_class_methods() {
    let source = r#"class DataManager {
    private data: string | null = null;
    private count: number | undefined = undefined;
    private active: boolean = true;

    ensureData(): string {
        this.data ??= "default-data";
        return this.data;
    }

    ensureCount(): number {
        this.count ||= 0;
        return this.count;
    }

    updateActive(value: boolean): boolean {
        this.active &&= value;
        return this.active;
    }

    reset(): void {
        this.data = null;
        this.count = undefined;
        this.active = true;
    }
}

class CacheManager {
    private cache: Map<string, string | null> = new Map();

    getOrSet(key: string, defaultValue: string): string {
        let value = this.cache.get(key);
        value ??= defaultValue;
        this.cache.set(key, value);
        return value;
    }
}

const manager = new DataManager();
console.log(manager.ensureData());
console.log(manager.ensureCount());
console.log(manager.updateActive(false));

const cache = new CacheManager();
console.log(cache.getOrSet("key", "value"));"#;

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
        output.contains("DataManager"),
        "expected DataManager in output. output: {output}"
    );
    assert!(
        output.contains("CacheManager"),
        "expected CacheManager in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test logical assignment with side effects with ES5 target
#[test]
fn test_source_map_logical_assignment_es5_side_effects() {
    let source = r#"let callCount = 0;

function getSideEffect(): string {
    callCount++;
    return "side-effect-value";
}

let value1: string | null = null;
let value2: string | null = "existing";

value1 ??= getSideEffect();
value2 ??= getSideEffect();

console.log("Call count:", callCount);

const obj = {
    _value: null as string | null,
    get value(): string | null {
        console.log("getter called");
        return this._value;
    },
    set value(v: string | null) {
        console.log("setter called");
        this._value = v;
    }
};

obj.value ??= "default";

function conditionalSideEffect(condition: boolean): string | null {
    if (condition) {
        return "truthy";
    }
    return null;
}

let sideEffectResult: string | null = null;
sideEffectResult ||= conditionalSideEffect(true);
console.log(value1, value2, sideEffectResult);"#;

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
        output.contains("getSideEffect"),
        "expected getSideEffect in output. output: {output}"
    );
    assert!(
        output.contains("callCount"),
        "expected callCount in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for side effects"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
