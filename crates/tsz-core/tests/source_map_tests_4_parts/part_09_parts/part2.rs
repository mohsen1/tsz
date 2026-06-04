/// Comprehensive test combining multiple type parameter constraint patterns.
/// Tests generic functions, classes, interfaces with various constraint types.
#[test]
fn test_source_map_type_constraint_es5_comprehensive() {
    let source = r#"// Base interfaces for constraints
interface Identifiable {
    id: string;
}

interface Timestamped {
    createdAt: Date;
    updatedAt: Date;
}

interface Validatable {
    validate(): boolean;
}

// Generic class with multiple constraints
class DataStore<T extends Identifiable & Timestamped> {
    private data: Map<string, T> = new Map();

    save(item: T): void {
        this.data.set(item.id, item);
    }

    find(id: string): T | undefined {
        return this.data.get(id);
    }

    findRecent(since: Date): T[] {
        return Array.from(this.data.values())
            .filter(item => item.updatedAt > since);
    }
}

// Generic function with constraint referencing another type parameter
function createValidator<
    T extends Validatable,
    TResult extends { valid: boolean; errors: string[] }
>(item: T, resultFactory: () => TResult): TResult {
    const result = resultFactory();
    result.valid = item.validate();
    return result;
}

// Class with constrained method type parameters
class Mapper<TSource extends object> {
    map<TTarget extends object>(
        source: TSource,
        mapper: (s: TSource) => TTarget
    ): TTarget {
        return mapper(source);
    }

    mapArray<TTarget extends object>(
        sources: TSource[],
        mapper: (s: TSource) => TTarget
    ): TTarget[] {
        return sources.map(mapper);
    }
}

// Conditional constraint pattern
type Constructor<T> = new (...args: any[]) => T;

function mixin<TBase extends Constructor<{}>>(Base: TBase) {
    return class extends Base {
        mixinProp = "mixed";
    };
}

// Usage
interface User extends Identifiable, Timestamped {
    name: string;
    email: string;
}

const store = new DataStore<User>();
const mapper = new Mapper<{ x: number }>();
const result = mapper.map({ x: 1 }, (s) => ({ y: s.x * 2 }));"#;

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
        output.contains("DataStore"),
        "expected DataStore class in output. output: {output}"
    );
    assert!(
        output.contains("createValidator"),
        "expected createValidator function in output. output: {output}"
    );
    assert!(
        output.contains("Mapper"),
        "expected Mapper class in output. output: {output}"
    );
    assert!(
        output.contains("mixin"),
        "expected mixin function in output. output: {output}"
    );
    assert!(
        output.contains("findRecent"),
        "expected findRecent method in output. output: {output}"
    );
    assert!(
        output.contains("mapArray"),
        "expected mapArray method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive type constraints"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for conditional types with infer keyword in ES5 output.
/// Validates that infer patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_infer_es5() {
    let source = r#"// Infer return type
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;

// Infer parameter types
type Parameters<T> = T extends (...args: infer P) => any ? P : never;

// Infer array element type
type ElementType<T> = T extends (infer E)[] ? E : never;

// Infer promise resolved type
type Awaited<T> = T extends Promise<infer U> ? Awaited<U> : T;

// Function using inferred types
function getReturnType<T extends (...args: any[]) => any>(
    fn: T
): ReturnType<T> | undefined {
    try {
        return fn() as ReturnType<T>;
    } catch {
        return undefined;
    }
}

function callWithArgs<T extends (...args: any[]) => any>(
    fn: T,
    ...args: Parameters<T>
): ReturnType<T> {
    return fn(...args);
}

const add = (a: number, b: number) => a + b;
const result = callWithArgs(add, 1, 2);"#;

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
        output.contains("getReturnType"),
        "expected getReturnType function in output. output: {output}"
    );
    assert!(
        output.contains("callWithArgs"),
        "expected callWithArgs function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional type with infer"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for distributive conditional types in ES5 output.
/// Validates that distributive conditional patterns generate proper source mappings.
#[test]
fn test_source_map_conditional_type_distributive_es5() {
    let source = r#"// Distributive conditional type
type ToArray<T> = T extends any ? T[] : never;

// Non-nullable extraction
type NonNullable<T> = T extends null | undefined ? never : T;

// Extract types from union
type Extract<T, U> = T extends U ? T : never;

// Exclude types from union
type Exclude<T, U> = T extends U ? never : T;

// Practical usage
type StringOrNumber = string | number | null | undefined;
type NonNullStringOrNumber = NonNullable<StringOrNumber>;
type OnlyStrings = Extract<StringOrNumber, string>;
type NoStrings = Exclude<StringOrNumber, string>;

function filterNonNull<T>(items: (T | null | undefined)[]): NonNullable<T>[] {
    return items.filter((item): item is NonNullable<T> => item != null);
}

function extractStrings(items: (string | number)[]): string[] {
    return items.filter((item): item is string => typeof item === "string");
}

const mixed = [1, "hello", null, 2, "world", undefined];
const nonNull = filterNonNull(mixed);
const strings = extractStrings([1, "a", 2, "b"]);"#;

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
        output.contains("filterNonNull"),
        "expected filterNonNull function in output. output: {output}"
    );
    assert!(
        output.contains("extractStrings"),
        "expected extractStrings function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for distributive conditional type"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
