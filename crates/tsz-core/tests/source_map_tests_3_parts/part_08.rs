#[test]
fn test_source_map_class_with_methods() {
    // Test class with instance methods
    let source = r#"class Calculator {
    private value: number;

    constructor(initial: number = 0) {
        this.value = initial;
    }

    add(n: number): this {
        this.value += n;
        return this;
    }

    subtract(n: number): this {
        this.value -= n;
        return this;
    }

    multiply(n: number): this {
        this.value *= n;
        return this;
    }

    getResult(): number {
        return this.value;
    }
}

const calc = new Calculator(10);
console.log(calc.add(5).multiply(2).getResult());"#;

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
        output.contains("Calculator") || output.contains("add"),
        "expected output to contain Calculator or add. output: {output}"
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

#[test]
fn test_source_map_class_static_members() {
    // Test class with static methods and properties
    let source = r#"class Counter {
    static count: number = 0;

    static increment(): void {
        Counter.count++;
    }

    static decrement(): void {
        Counter.count--;
    }

    static getCount(): number {
        return Counter.count;
    }

    static reset(): void {
        Counter.count = 0;
    }
}

Counter.increment();
Counter.increment();
console.log(Counter.getCount());
Counter.reset();"#;

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
        output.contains("Counter") || output.contains("increment"),
        "expected output to contain Counter or increment. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static members"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_getters_setters() {
    // Test class with getter and setter accessors
    let source = r#"class Person {
    private _firstName: string;
    private _lastName: string;
    private _age: number;

    constructor(firstName: string, lastName: string, age: number) {
        this._firstName = firstName;
        this._lastName = lastName;
        this._age = age;
    }

    get fullName(): string {
        return this._firstName + " " + this._lastName;
    }

    set fullName(value: string) {
        const parts = value.split(" ");
        this._firstName = parts[0] || "";
        this._lastName = parts[1] || "";
    }

    get age(): number {
        return this._age;
    }

    set age(value: number) {
        if (value >= 0) {
            this._age = value;
        }
    }
}

const person = new Person("John", "Doe", 30);
console.log(person.fullName);
person.fullName = "Jane Smith";
console.log(person.age);"#;

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
        output.contains("Person") || output.contains("fullName"),
        "expected output to contain Person or fullName. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for getters/setters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance() {
    // Test class inheritance with extends
    let source = r#"class Shape {
    protected x: number;
    protected y: number;

    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }

    move(dx: number, dy: number): void {
        this.x += dx;
        this.y += dy;
    }

    describe(): string {
        return "Shape at (" + this.x + ", " + this.y + ")";
    }
}

class Circle extends Shape {
    private radius: number;

    constructor(x: number, y: number, radius: number) {
        super(x, y);
        this.radius = radius;
    }

    describe(): string {
        return "Circle at (" + this.x + ", " + this.y + ") with radius " + this.radius;
    }

    area(): number {
        return Math.PI * this.radius * this.radius;
    }
}

class Rectangle extends Shape {
    private width: number;
    private height: number;

    constructor(x: number, y: number, width: number, height: number) {
        super(x, y);
        this.width = width;
        this.height = height;
    }

    describe(): string {
        return "Rectangle at (" + this.x + ", " + this.y + ")";
    }

    area(): number {
        return this.width * this.height;
    }
}

const circle = new Circle(0, 0, 5);
const rect = new Rectangle(10, 10, 20, 30);
console.log(circle.describe());
console.log(rect.area());"#;

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
        output.contains("Shape") || output.contains("Circle") || output.contains("Rectangle"),
        "expected output to contain Shape, Circle, or Rectangle. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_constructor_parameter_properties() {
    // Test class with constructor parameter properties
    let source = r#"class User {
    constructor(
        public readonly id: number,
        public name: string,
        private email: string,
        protected role: string = "user"
    ) {}

    getEmail(): string {
        return this.email;
    }

    describe(): string {
        return "User " + this.name + " with role " + this.role;
    }
}

class Admin extends User {
    constructor(id: number, name: string, email: string) {
        super(id, name, email, "admin");
    }

    getRole(): string {
        return this.role;
    }
}

const user = new User(1, "John", "john@example.com");
const admin = new Admin(2, "Jane", "jane@example.com");
console.log(user.describe());
console.log(admin.getRole());"#;

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
        output.contains("User") || output.contains("Admin"),
        "expected output to contain User or Admin. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for constructor parameter properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expression() {
    // Test class expressions (anonymous and named)
    let source = r#"const Logger = class {
    private prefix: string;

    constructor(prefix: string) {
        this.prefix = prefix;
    }

    log(message: string): void {
        console.log(this.prefix + ": " + message);
    }
};

const NamedLogger = class CustomLogger {
    private level: string;

    constructor(level: string) {
        this.level = level;
    }

    log(message: string): void {
        console.log("[" + this.level + "] " + message);
    }
};

const factories = {
    createLogger: class {
        create(name: string) {
            return new Logger(name);
        }
    }
};

const logger = new Logger("App");
const namedLogger = new NamedLogger("INFO");
logger.log("Hello");
namedLogger.log("World");"#;

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
        output.contains("Logger") || output.contains("log"),
        "expected output to contain Logger or log. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_generic() {
    // Test generic class declarations
    let source = r#"class Container<T> {
    private value: T;

    constructor(value: T) {
        this.value = value;
    }

    getValue(): T {
        return this.value;
    }

    setValue(value: T): void {
        this.value = value;
    }
}

class Pair<K, V> {
    constructor(private key: K, private val: V) {}

    getKey(): K {
        return this.key;
    }

    getValue(): V {
        return this.val;
    }

    toArray(): [K, V] {
        return [this.key, this.val];
    }
}

class Stack<T> {
    private items: T[] = [];

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T | undefined {
        return this.items.pop();
    }

    peek(): T | undefined {
        return this.items[this.items.length - 1];
    }

    isEmpty(): boolean {
        return this.items.length === 0;
    }
}

const numContainer = new Container<number>(42);
const strPair = new Pair<string, number>("age", 30);
const stack = new Stack<string>();
stack.push("hello");
console.log(numContainer.getValue());
console.log(strPair.toArray());"#;

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
        output.contains("Container") || output.contains("Stack") || output.contains("Pair"),
        "expected output to contain Container, Stack, or Pair. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic classes"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_abstract() {
    // Test abstract class declarations
    let source = r#"abstract class Vehicle {
    protected speed: number = 0;

    constructor(protected name: string) {}

    abstract start(): void;
    abstract stop(): void;

    accelerate(amount: number): void {
        this.speed += amount;
    }

    getSpeed(): number {
        return this.speed;
    }

    describe(): string {
        return this.name + " moving at " + this.speed;
    }
}

class Car extends Vehicle {
    constructor(name: string) {
        super(name);
    }

    start(): void {
        console.log(this.name + " engine started");
        this.speed = 10;
    }

    stop(): void {
        console.log(this.name + " stopped");
        this.speed = 0;
    }
}

class Bicycle extends Vehicle {
    constructor(name: string) {
        super(name);
    }

    start(): void {
        console.log("Pedaling " + this.name);
        this.speed = 5;
    }

    stop(): void {
        console.log("Braking " + this.name);
        this.speed = 0;
    }
}

const car = new Car("Tesla");
const bike = new Bicycle("Mountain Bike");
car.start();
car.accelerate(50);
console.log(car.describe());
bike.start();
console.log(bike.getSpeed());"#;

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
        output.contains("Vehicle") || output.contains("Car") || output.contains("Bicycle"),
        "expected output to contain Vehicle, Car, or Bicycle. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for abstract classes"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_declaration_combined() {
    // Test combined class declaration patterns
    let source = r#"abstract class BaseService<T> {
    protected items: T[] = [];

    abstract validate(item: T): boolean;

    add(item: T): void {
        if (this.validate(item)) {
            this.items.push(item);
        }
    }

    getAll(): T[] {
        return this.items.slice();
    }

    get count(): number {
        return this.items.length;
    }
}

interface Entity {
    id: number;
    name: string;
}

class EntityService extends BaseService<Entity> {
    private static instance: EntityService;
    static counter: number = 0;

    private constructor() {
        super();
    }

    static getInstance(): EntityService {
        if (!EntityService.instance) {
            EntityService.instance = new EntityService();
        }
        return EntityService.instance;
    }

    validate(item: Entity): boolean {
        EntityService.counter++;
        return item.id > 0 && item.name.length > 0;
    }

    findById(id: number): Entity | undefined {
        return this.items.find(item => item.id === id);
    }

    get isEmpty(): boolean {
        return this.items.length === 0;
    }

    set defaultItem(item: Entity) {
        if (this.isEmpty) {
            this.add(item);
        }
    }
}

class CachedService<T extends Entity> extends BaseService<T> {
    private cache: Map<number, T> = new Map();

    constructor(private readonly cacheDuration: number = 1000) {
        super();
    }

    validate(item: T): boolean {
        return item.id > 0;
    }

    add(item: T): void {
        super.add(item);
        this.cache.set(item.id, item);
    }

    getFromCache(id: number): T | undefined {
        return this.cache.get(id);
    }
}

const service = EntityService.getInstance();
service.add({ id: 1, name: "First" });
service.add({ id: 2, name: "Second" });
console.log(service.count);
console.log(EntityService.counter);

const cached = new CachedService<Entity>(5000);
cached.add({ id: 100, name: "Cached Item" });
console.log(cached.getFromCache(100));"#;

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
        output.contains("BaseService")
            || output.contains("EntityService")
            || output.contains("CachedService"),
        "expected output to contain BaseService, EntityService, or CachedService. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined class patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Interface/Type Alias ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_basic() {
    // Test basic interface with type erasure - interface is removed, runtime code mapped
    let source = r#"interface Person {
    name: string;
    age: number;
}

const person: Person = {
    name: "John",
    age: 30
};

console.log(person.name);"#;

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

    // Interface should be erased, but runtime code should be present
    assert!(
        !output.contains("interface"),
        "interface keyword should be erased. output: {output}"
    );
    assert!(
        output.contains("person"),
        "expected output to contain person. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface usage"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

