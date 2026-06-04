#[test]
fn test_source_map_symbol_es5_comprehensive() {
    let source = r#"// Comprehensive Symbol-keyed member test

const customKey = Symbol("customKey");

class SuperCollection<T> {
    protected items: T[] = [];
    protected name: string;

    constructor(name: string) {
        this.name = name;
    }

    // Symbol.toStringTag
    get [Symbol.toStringTag](): string {
        return `SuperCollection<${this.name}>`;
    }

    // Symbol.iterator
    *[Symbol.iterator](): Iterator<T> {
        yield* this.items;
    }

    // Symbol.toPrimitive
    [Symbol.toPrimitive](hint: string): string | number {
        if (hint === "number") {
            return this.items.length;
        }
        return `[${this.name}: ${this.items.length} items]`;
    }

    // Custom symbol method
    [customKey](multiplier: number): number {
        return this.items.length * multiplier;
    }

    // Symbol.hasInstance
    static [Symbol.hasInstance](instance: any): boolean {
        return instance !== null &&
               typeof instance === "object" &&
               "items" in instance &&
               "name" in instance;
    }

    add(...items: T[]): this {
        this.items.push(...items);
        return this;
    }

    get size(): number {
        return this.items.length;
    }
}

class AsyncSuperCollection<T> extends SuperCollection<T> {
    // Symbol.asyncIterator
    async *[Symbol.asyncIterator](): AsyncIterator<T> {
        for (const item of this.items) {
            await new Promise(r => setTimeout(r, 10));
            yield item;
        }
    }

    // Override Symbol.toStringTag
    get [Symbol.toStringTag](): string {
        return `AsyncSuperCollection<${this.name}>`;
    }

    // Symbol.species
    static get [Symbol.species](): typeof AsyncSuperCollection {
        return AsyncSuperCollection;
    }

    async processAll<U>(fn: (item: T) => Promise<U>): Promise<U[]> {
        const results: U[] = [];
        for await (const item of this) {
            results.push(await fn(item));
        }
        return results;
    }
}

// Usage
const collection = new SuperCollection<number>("Numbers");
collection.add(1, 2, 3, 4, 5);

console.log(Object.prototype.toString.call(collection));
console.log([...collection]);
console.log(+collection);
console.log(`${collection}`);
console.log(collection[customKey](10));
console.log({ items: [], name: "test" } instanceof SuperCollection);

const asyncCollection = new AsyncSuperCollection<string>("Strings");
asyncCollection.add("a", "b", "c");

(async () => {
    for await (const item of asyncCollection) {
        console.log(item);
    }

    const results = await asyncCollection.processAll(async (s) => s.toUpperCase());
    console.log(results);
})();"#;

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
        output.contains("SuperCollection"),
        "expected output to contain SuperCollection class. output: {output}"
    );
    assert!(
        output.contains("AsyncSuperCollection"),
        "expected output to contain AsyncSuperCollection class. output: {output}"
    );
    assert!(
        output.contains("customKey"),
        "expected output to contain customKey symbol. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive Symbol-keyed members"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_reflect_metadata() {
    let source = r#"// Simulating reflect-metadata patterns
const metadataKey = Symbol("metadata");

function Metadata(key: string, value: any): ClassDecorator & MethodDecorator & PropertyDecorator {
    return function(target: any, propertyKey?: string | symbol, descriptor?: PropertyDescriptor) {
        if (propertyKey === undefined) {
            // Class decorator
            Reflect.defineMetadata(key, value, target);
        } else {
            // Method or property decorator
            Reflect.defineMetadata(key, value, target, propertyKey);
        }
        return descriptor as any;
    };
}

function getMetadata(key: string, target: any, propertyKey?: string | symbol): any {
    if (propertyKey === undefined) {
        return Reflect.getMetadata(key, target);
    }
    return Reflect.getMetadata(key, target, propertyKey);
}

@Metadata("role", "admin")
@Metadata("version", "1.0")
class UserService {
    @Metadata("column", "user_name")
    name: string = "";

    @Metadata("endpoint", "/users")
    @Metadata("method", "GET")
    getUsers(): string[] {
        return [];
    }
}

const service = new UserService();
console.log(getMetadata("role", UserService));
console.log(getMetadata("column", service, "name"));"#;

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
        output.contains("UserService"),
        "expected output to contain UserService class. output: {output}"
    );
    assert!(
        output.contains("Metadata"),
        "expected output to contain Metadata decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for reflect-metadata"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_decorator_metadata_es5_parameter_decorators() {
    let source = r#"const paramMetadata = new Map<string, Map<number, any>>();

function Inject(token: string): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey ? String(propertyKey) : "constructor";
        if (!paramMetadata.has(key)) {
            paramMetadata.set(key, new Map());
        }
        paramMetadata.get(key)!.set(parameterIndex, { token });
    };
}

function Required(): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey ? String(propertyKey) : "constructor";
        if (!paramMetadata.has(key)) {
            paramMetadata.set(key, new Map());
        }
        const existing = paramMetadata.get(key)!.get(parameterIndex) || {};
        paramMetadata.get(key)!.set(parameterIndex, { ...existing, required: true });
    };
}

function Validate(validator: (val: any) => boolean): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, parameterIndex: number) {
        const key = propertyKey ? String(propertyKey) : "constructor";
        if (!paramMetadata.has(key)) {
            paramMetadata.set(key, new Map());
        }
        const existing = paramMetadata.get(key)!.get(parameterIndex) || {};
        paramMetadata.get(key)!.set(parameterIndex, { ...existing, validator });
    };
}

class ApiController {
    constructor(
        @Inject("HttpClient") private http: any,
        @Inject("Logger") @Required() private logger: any
    ) {}

    fetchData(
        @Required() @Validate(v => typeof v === "string") endpoint: string,
        @Inject("Cache") cache?: any
    ): Promise<any> {
        return this.http.get(endpoint);
    }
}

const controller = new ApiController({}, {});
console.log(paramMetadata);"#;

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
        output.contains("ApiController"),
        "expected output to contain ApiController class. output: {output}"
    );
    assert!(
        output.contains("Inject"),
        "expected output to contain Inject decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for parameter decorators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
