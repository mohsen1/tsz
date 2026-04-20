#[test]
fn test_source_map_symbol_es5_computed_symbol_methods() {
    let source = r#"const customMethod = Symbol("customMethod");
const customGetter = Symbol("customGetter");
const customProperty = Symbol("customProperty");

class SymbolClass {
    [customProperty]: string = "default";

    [customMethod](x: number, y: number): number {
        return x + y;
    }

    get [customGetter](): string {
        return `Value: ${this[customProperty]}`;
    }

    set [customGetter](value: string) {
        this[customProperty] = value;
    }
}

const obj = new SymbolClass();
console.log(obj[customMethod](1, 2));
console.log(obj[customGetter]);
obj[customGetter] = "updated";
console.log(obj[customProperty]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        output.contains("SymbolClass"),
        "expected output to contain SymbolClass. output: {output}"
    );
    assert!(
        output.contains("customMethod"),
        "expected output to contain customMethod. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed Symbol methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_to_string_tag() {
    let source = r#"class CustomCollection<T> {
    private items: T[] = [];

    get [Symbol.toStringTag](): string {
        return "CustomCollection";
    }

    add(item: T): void {
        this.items.push(item);
    }

    get size(): number {
        return this.items.length;
    }
}

class NamedObject {
    private name: string;

    constructor(name: string) {
        this.name = name;
    }

    get [Symbol.toStringTag](): string {
        return `NamedObject(${this.name})`;
    }
}

const collection = new CustomCollection<number>();
collection.add(1);
collection.add(2);
console.log(Object.prototype.toString.call(collection));

const named = new NamedObject("test");
console.log(Object.prototype.toString.call(named));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        output.contains("CustomCollection"),
        "expected output to contain CustomCollection class. output: {output}"
    );
    assert!(
        output.contains("NamedObject"),
        "expected output to contain NamedObject class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.toStringTag"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_has_instance() {
    let source = r#"class CustomType {
    private value: any;

    constructor(value: any) {
        this.value = value;
    }

    static [Symbol.hasInstance](instance: any): boolean {
        return instance !== null &&
               typeof instance === "object" &&
               "value" in instance;
    }
}

class ExtendedCustomType extends CustomType {
    private extra: string;

    constructor(value: any, extra: string) {
        super(value);
        this.extra = extra;
    }

    static [Symbol.hasInstance](instance: any): boolean {
        return super[Symbol.hasInstance](instance) && "extra" in instance;
    }
}

const obj1 = new CustomType(42);
const obj2 = new ExtendedCustomType(42, "hello");
const obj3 = { value: 10 };

console.log(obj1 instanceof CustomType);
console.log(obj2 instanceof CustomType);
console.log(obj3 instanceof CustomType);
console.log(obj2 instanceof ExtendedCustomType);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        output.contains("CustomType"),
        "expected output to contain CustomType class. output: {output}"
    );
    assert!(
        output.contains("ExtendedCustomType"),
        "expected output to contain ExtendedCustomType class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.hasInstance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_species() {
    let source = r#"class MyArray<T> extends Array<T> {
    static get [Symbol.species](): ArrayConstructor {
        return Array;
    }

    customMethod(): T | undefined {
        return this[0];
    }
}

class SpecialArray<T> extends Array<T> {
    static get [Symbol.species](): typeof SpecialArray {
        return SpecialArray;
    }

    static create<U>(...items: U[]): SpecialArray<U> {
        const arr = new SpecialArray<U>();
        arr.push(...items);
        return arr;
    }

    double(): SpecialArray<T> {
        return this.concat(this) as SpecialArray<T>;
    }
}

const myArr = new MyArray(1, 2, 3);
const mapped = myArr.map(x => x * 2);
console.log(mapped instanceof MyArray);
console.log(mapped instanceof Array);

const special = SpecialArray.create(1, 2, 3);
const doubled = special.double();
console.log(doubled instanceof SpecialArray);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        output.contains("MyArray"),
        "expected output to contain MyArray class. output: {output}"
    );
    assert!(
        output.contains("SpecialArray"),
        "expected output to contain SpecialArray class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.species"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_to_primitive() {
    let source = r#"class Money {
    private amount: number;
    private currency: string;

    constructor(amount: number, currency: string = "USD") {
        this.amount = amount;
        this.currency = currency;
    }

    [Symbol.toPrimitive](hint: string): string | number {
        if (hint === "number") {
            return this.amount;
        }
        if (hint === "string") {
            return `${this.currency} ${this.amount.toFixed(2)}`;
        }
        return this.amount;
    }

    add(other: Money): Money {
        if (this.currency !== other.currency) {
            throw new Error("Currency mismatch");
        }
        return new Money(this.amount + other.amount, this.currency);
    }
}

const price = new Money(99.99);
const tax = new Money(8.50);
console.log(+price);
console.log(`${price}`);
console.log(price + 0);
const total = price.add(tax);
console.log(`${total}`);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        output.contains("Money"),
        "expected output to contain Money class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.toPrimitive"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_is_concat_spreadable() {
    let source = r#"class SpreadableCollection<T> {
    private items: T[];

    constructor(...items: T[]) {
        this.items = items;
    }

    get [Symbol.isConcatSpreadable](): boolean {
        return true;
    }

    get length(): number {
        return this.items.length;
    }

    [index: number]: T;

    *[Symbol.iterator](): Iterator<T> {
        yield* this.items;
    }
}

// Set up indexed access
const createSpreadable = <T>(...items: T[]): SpreadableCollection<T> & T[] => {
    const collection = new SpreadableCollection(...items);
    items.forEach((item, i) => {
        (collection as any)[i] = item;
    });
    return collection as SpreadableCollection<T> & T[];
};

const arr1 = [1, 2, 3];
const spreadable = createSpreadable(4, 5, 6);
const combined = arr1.concat(spreadable);
console.log(combined);
console.log(combined.length);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        output.contains("SpreadableCollection"),
        "expected output to contain SpreadableCollection class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.isConcatSpreadable"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

// =============================================================================
// Decorator Metadata ES5 Source Map Tests
// =============================================================================

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

#[test]
fn test_source_map_decorator_metadata_es5_property_descriptors() {
    let source = r#"function Observable(): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        let value: any;
        const getter = function(this: any) {
            console.log(`Getting ${String(propertyKey)}`);
            return value;
        };
        const setter = function(this: any, newVal: any) {
            console.log(`Setting ${String(propertyKey)} to ${newVal}`);
            value = newVal;
        };
        Object.defineProperty(target, propertyKey, {
            get: getter,
            set: setter,
            enumerable: true,
            configurable: true
        });
    };
}

function DefaultValue(defaultVal: any): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        let value = defaultVal;
        Object.defineProperty(target, propertyKey, {
            get() { return value; },
            set(newVal) { value = newVal; },
            enumerable: true,
            configurable: true
        });
    };
}

function Readonly(): PropertyDecorator {
    return function(target: Object, propertyKey: string | symbol) {
        Object.defineProperty(target, propertyKey, {
            writable: false,
            configurable: false
        });
    };
}

class Config {
    @Observable()
    @DefaultValue("development")
    environment: string;

    @Observable()
    @DefaultValue(3000)
    port: number;

    @Readonly()
    version: string = "1.0.0";
}

const config = new Config();
console.log(config.environment);
config.port = 8080;
console.log(config.port);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
        output.contains("Config"),
        "expected output to contain Config class. output: {output}"
    );
    assert!(
        output.contains("Observable"),
        "expected output to contain Observable decorator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property descriptors"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

