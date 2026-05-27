#[test]
fn test_source_map_private_field_es5_private_method_calls() {
    let source = r#"class Calculator {
    #value: number = 0;

    #add(n: number): void {
        this.#value += n;
    }

    #subtract(n: number): void {
        this.#value -= n;
    }

    #multiply(n: number): void {
        this.#value *= n;
    }

    #divide(n: number): void {
        if (n !== 0) {
            this.#value /= n;
        }
    }

    #validate(n: number): boolean {
        return typeof n === 'number' && !isNaN(n);
    }

    add(n: number): this {
        if (this.#validate(n)) {
            this.#add(n);
        }
        return this;
    }

    subtract(n: number): this {
        if (this.#validate(n)) {
            this.#subtract(n);
        }
        return this;
    }

    multiply(n: number): this {
        if (this.#validate(n)) {
            this.#multiply(n);
        }
        return this;
    }

    divide(n: number): this {
        if (this.#validate(n)) {
            this.#divide(n);
        }
        return this;
    }

    getValue(): number {
        return this.#value;
    }
}

const calc = new Calculator();
const result = calc.add(10).multiply(2).subtract(5).divide(3).getValue();
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
        output.contains("Calculator"),
        "expected output to contain Calculator class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private method calls"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_accessor_patterns() {
    let source = r#"class Person {
    #firstName: string;
    #lastName: string;
    #age: number;

    constructor(firstName: string, lastName: string, age: number) {
        this.#firstName = firstName;
        this.#lastName = lastName;
        this.#age = age;
    }

    get firstName(): string {
        return this.#firstName;
    }

    set firstName(value: string) {
        this.#firstName = value.trim();
    }

    get lastName(): string {
        return this.#lastName;
    }

    set lastName(value: string) {
        this.#lastName = value.trim();
    }

    get fullName(): string {
        return `${this.#firstName} ${this.#lastName}`;
    }

    set fullName(value: string) {
        const parts = value.split(' ');
        this.#firstName = parts[0] || '';
        this.#lastName = parts.slice(1).join(' ') || '';
    }

    get age(): number {
        return this.#age;
    }

    set age(value: number) {
        if (value >= 0 && value <= 150) {
            this.#age = value;
        }
    }
}

const person = new Person("John", "Doe", 30);
console.log(person.fullName);
person.fullName = "Jane Smith";
console.log(person.firstName, person.lastName);
person.age = 25;
console.log(person.age);"#;

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
        output.contains("Person"),
        "expected output to contain Person class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private accessor patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_derived_class() {
    let source = r#"class Animal {
    #name: string;
    #species: string;

    constructor(name: string, species: string) {
        this.#name = name;
        this.#species = species;
    }

    getName(): string {
        return this.#name;
    }

    getSpecies(): string {
        return this.#species;
    }

    describe(): string {
        return `${this.#name} is a ${this.#species}`;
    }
}

class Dog extends Animal {
    #breed: string;
    #trained: boolean;

    constructor(name: string, breed: string) {
        super(name, "dog");
        this.#breed = breed;
        this.#trained = false;
    }

    getBreed(): string {
        return this.#breed;
    }

    train(): void {
        this.#trained = true;
    }

    isTrained(): boolean {
        return this.#trained;
    }

    describe(): string {
        const base = super.describe();
        return `${base} (${this.#breed}, trained: ${this.#trained})`;
    }
}

class Cat extends Animal {
    #indoor: boolean;

    constructor(name: string, indoor: boolean = true) {
        super(name, "cat");
        this.#indoor = indoor;
    }

    isIndoor(): boolean {
        return this.#indoor;
    }

    describe(): string {
        const base = super.describe();
        return `${base} (${this.#indoor ? "indoor" : "outdoor"})`;
    }
}

const dog = new Dog("Buddy", "Labrador");
dog.train();
console.log(dog.describe());

const cat = new Cat("Whiskers", false);
console.log(cat.describe());"#;

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
        output.contains("Animal"),
        "expected output to contain Animal class. output: {output}"
    );
    assert!(
        output.contains("Dog"),
        "expected output to contain Dog class. output: {output}"
    );
    assert!(
        output.contains("Cat"),
        "expected output to contain Cat class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field in derived class"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_weakmap_polyfill() {
    let source = r#"class SecureStorage {
    #data: Map<string, any> = new Map();
    #encryptionKey: string;
    static #instances: WeakMap<object, SecureStorage> = new WeakMap();

    constructor(key: string) {
        this.#encryptionKey = key;
        SecureStorage.#instances.set(this, this);
    }

    #encrypt(value: string): string {
        return btoa(value + this.#encryptionKey);
    }

    #decrypt(value: string): string {
        const decoded = atob(value);
        return decoded.replace(this.#encryptionKey, '');
    }

    set(key: string, value: any): void {
        const encrypted = this.#encrypt(JSON.stringify(value));
        this.#data.set(key, encrypted);
    }

    get(key: string): any {
        const encrypted = this.#data.get(key);
        if (encrypted) {
            return JSON.parse(this.#decrypt(encrypted));
        }
        return undefined;
    }

    has(key: string): boolean {
        return this.#data.has(key);
    }

    delete(key: string): boolean {
        return this.#data.delete(key);
    }

    static getInstance(obj: object): SecureStorage | undefined {
        return SecureStorage.#instances.get(obj);
    }
}

const storage = new SecureStorage("secret123");
storage.set("user", { name: "John", role: "admin" });
console.log(storage.get("user"));
console.log(storage.has("user"));
console.log(SecureStorage.getInstance(storage));"#;

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
        output.contains("SecureStorage"),
        "expected output to contain SecureStorage class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for WeakMap polyfill"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_in_check() {
    let source = r#"class BrandedClass {
    #brand: symbol = Symbol("branded");

    static isBranded(obj: any): boolean {
        return #brand in obj;
    }

    getBrand(): symbol {
        return this.#brand;
    }
}

class Container<T> {
    #value: T;
    #initialized: boolean = false;

    constructor(value: T) {
        this.#value = value;
        this.#initialized = true;
    }

    static isContainer(obj: any): boolean {
        return #value in obj && #initialized in obj;
    }

    getValue(): T {
        return this.#value;
    }

    setValue(value: T): void {
        this.#value = value;
    }
}

const branded = new BrandedClass();
console.log(BrandedClass.isBranded(branded));
console.log(BrandedClass.isBranded({}));

const container = new Container<number>(42);
console.log(Container.isContainer(container));
console.log(container.getValue());"#;

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
        output.contains("BrandedClass"),
        "expected output to contain BrandedClass. output: {output}"
    );
    assert!(
        output.contains("Container"),
        "expected output to contain Container. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field in-check"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_static_method() {
    let source = r#"class Singleton {
    static #instance: Singleton | null = null;
    #id: number;

    private constructor() {
        this.#id = Math.random();
    }

    static #createInstance(): Singleton {
        return new Singleton();
    }

    static getInstance(): Singleton {
        if (Singleton.#instance === null) {
            Singleton.#instance = Singleton.#createInstance();
        }
        return Singleton.#instance;
    }

    static #resetInstance(): void {
        Singleton.#instance = null;
    }

    static reset(): void {
        Singleton.#resetInstance();
    }

    getId(): number {
        return this.#id;
    }
}

const instance1 = Singleton.getInstance();
const instance2 = Singleton.getInstance();
console.log(instance1 === instance2);
console.log(instance1.getId());
Singleton.reset();
const instance3 = Singleton.getInstance();
console.log(instance1 === instance3);"#;

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
        output.contains("Singleton"),
        "expected output to contain Singleton class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private static method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_private_field_es5_comprehensive() {
    let source = r#"// Comprehensive private field test with all patterns

class EventEmitter<T extends Record<string, any[]>> {
    #listeners: Map<keyof T, Set<(...args: any[]) => void>> = new Map();
    #maxListeners: number = 10;
    static #globalEmitters: WeakMap<object, EventEmitter<any>> = new WeakMap();

    constructor() {
        EventEmitter.#globalEmitters.set(this, this);
    }

    #getListeners<K extends keyof T>(event: K): Set<(...args: T[K]) => void> {
        let listeners = this.#listeners.get(event);
        if (!listeners) {
            listeners = new Set();
            this.#listeners.set(event, listeners);
        }
        return listeners as Set<(...args: T[K]) => void>;
    }

    #checkMaxListeners(event: keyof T): void {
        const count = this.#getListeners(event).size;
        if (count > this.#maxListeners) {
            console.warn(`Max listeners exceeded for event: ${String(event)}`);
        }
    }

    on<K extends keyof T>(event: K, listener: (...args: T[K]) => void): this {
        this.#getListeners(event).add(listener);
        this.#checkMaxListeners(event);
        return this;
    }

    off<K extends keyof T>(event: K, listener: (...args: T[K]) => void): this {
        this.#getListeners(event).delete(listener);
        return this;
    }

    emit<K extends keyof T>(event: K, ...args: T[K]): boolean {
        const listeners = this.#getListeners(event);
        if (listeners.size === 0) return false;
        listeners.forEach(listener => listener(...args));
        return true;
    }

    get maxListeners(): number {
        return this.#maxListeners;
    }

    set maxListeners(value: number) {
        this.#maxListeners = Math.max(0, value);
    }

    static isEmitter(obj: any): boolean {
        return #listeners in obj;
    }

    static getEmitter(obj: object): EventEmitter<any> | undefined {
        return EventEmitter.#globalEmitters.get(obj);
    }
}

class TypedEventEmitter extends EventEmitter<{
    connect: [host: string, port: number];
    disconnect: [reason: string];
    message: [data: string, timestamp: Date];
}> {
    #connected: boolean = false;
    #host: string = "";
    #port: number = 0;

    async #doConnect(host: string, port: number): Promise<void> {
        await new Promise(r => setTimeout(r, 100));
        this.#host = host;
        this.#port = port;
        this.#connected = true;
    }

    async connect(host: string, port: number): Promise<void> {
        await this.#doConnect(host, port);
        this.emit("connect", host, port);
    }

    disconnect(reason: string): void {
        this.#connected = false;
        this.emit("disconnect", reason);
    }

    send(data: string): void {
        if (this.#connected) {
            this.emit("message", data, new Date());
        }
    }

    get isConnected(): boolean {
        return this.#connected;
    }

    get connectionInfo(): { host: string; port: number } | null {
        if (this.#connected) {
            return { host: this.#host, port: this.#port };
        }
        return null;
    }
}

// Usage
const emitter = new TypedEventEmitter();

emitter.on("connect", (host, port) => {
    console.log(`Connected to ${host}:${port}`);
});

emitter.on("message", (data, timestamp) => {
    console.log(`[${timestamp.toISOString()}] ${data}`);
});

emitter.on("disconnect", (reason) => {
    console.log(`Disconnected: ${reason}`);
});

emitter.connect("localhost", 8080).then(() => {
    emitter.send("Hello, World!");
    emitter.disconnect("User requested");
});

console.log(EventEmitter.isEmitter(emitter));
console.log(EventEmitter.getEmitter(emitter) === emitter);"#;

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
        output.contains("EventEmitter"),
        "expected output to contain EventEmitter class. output: {output}"
    );
    assert!(
        output.contains("TypedEventEmitter"),
        "expected output to contain TypedEventEmitter class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive private field"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Symbol-keyed Member ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_symbol_es5_iterator() {
    let source = r#"class Range {
    private start: number;
    private end: number;
    private step: number;

    constructor(start: number, end: number, step: number = 1) {
        this.start = start;
        this.end = end;
        this.step = step;
    }

    *[Symbol.iterator](): Iterator<number> {
        for (let i = this.start; i < this.end; i += this.step) {
            yield i;
        }
    }

    toArray(): number[] {
        return [...this];
    }
}

const range = new Range(0, 10, 2);
for (const n of range) {
    console.log(n);
}
console.log(range.toArray());"#;

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
        output.contains("Range"),
        "expected output to contain Range class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.iterator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_symbol_es5_async_iterator() {
    let source = r#"class AsyncQueue<T> {
    private items: T[] = [];
    private resolvers: ((value: IteratorResult<T>) => void)[] = [];
    private done: boolean = false;

    push(item: T): void {
        if (this.resolvers.length > 0) {
            const resolve = this.resolvers.shift()!;
            resolve({ value: item, done: false });
        } else {
            this.items.push(item);
        }
    }

    close(): void {
        this.done = true;
        for (const resolve of this.resolvers) {
            resolve({ value: undefined as any, done: true });
        }
        this.resolvers = [];
    }

    async *[Symbol.asyncIterator](): AsyncIterator<T> {
        while (!this.done || this.items.length > 0) {
            if (this.items.length > 0) {
                yield this.items.shift()!;
            } else if (!this.done) {
                yield await new Promise<T>((resolve) => {
                    this.resolvers.push((result) => {
                        if (!result.done) {
                            resolve(result.value);
                        }
                    });
                });
            }
        }
    }
}

const queue = new AsyncQueue<number>();
queue.push(1);
queue.push(2);
queue.push(3);
queue.close();

(async () => {
    for await (const item of queue) {
        console.log(item);
    }
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
        output.contains("AsyncQueue"),
        "expected output to contain AsyncQueue class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for Symbol.asyncIterator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

