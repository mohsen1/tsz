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

#[test]
fn test_source_map_type_alias_basic() {
    // Test basic type alias with type erasure
    let source = r#"type StringOrNumber = string | number;
type Point = { x: number; y: number };

const value: StringOrNumber = "hello";
const point: Point = { x: 10, y: 20 };

console.log(value, point.x);"#;

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

    // Type alias should be erased
    assert!(
        !output.contains("type StringOrNumber") && !output.contains("type Point"),
        "type alias should be erased. output: {output}"
    );
    assert!(
        output.contains("value") && output.contains("point"),
        "expected output to contain value and point. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type alias usage"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_with_methods() {
    // Test interface with method signatures
    let source = r#"interface Calculator {
    add(a: number, b: number): number;
    subtract(a: number, b: number): number;
    multiply(a: number, b: number): number;
}

const calc: Calculator = {
    add(a, b) { return a + b; },
    subtract(a, b) { return a - b; },
    multiply(a, b) { return a * b; }
};

console.log(calc.add(5, 3));
console.log(calc.subtract(10, 4));
console.log(calc.multiply(2, 6));"#;

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
        output.contains("calc") && output.contains("add"),
        "expected output to contain calc and add. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface with methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_extends() {
    // Test interface extending another interface
    let source = r#"interface Animal {
    name: string;
    age: number;
}

interface Dog extends Animal {
    breed: string;
    bark(): void;
}

const dog: Dog = {
    name: "Rex",
    age: 5,
    breed: "German Shepherd",
    bark() {
        console.log("Woof!");
    }
};

dog.bark();
console.log(dog.name, dog.breed);"#;

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
        output.contains("dog") && output.contains("bark"),
        "expected output to contain dog and bark. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface extends"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_union_intersection() {
    // Test union and intersection type aliases
    let source = r#"type ID = string | number;
type Name = { first: string; last: string };
type Age = { age: number };
type Person = Name & Age;

const id: ID = 123;
const person: Person = {
    first: "John",
    last: "Doe",
    age: 30
};

function printId(id: ID): void {
    console.log("ID:", id);
}

function printPerson(p: Person): void {
    console.log(p.first, p.last, p.age);
}

printId(id);
printPerson(person);"#;

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
        output.contains("printId") && output.contains("printPerson"),
        "expected output to contain printId and printPerson. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for union/intersection types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_generic() {
    // Test generic interface declarations
    let source = r#"interface Container<T> {
    value: T;
    getValue(): T;
    setValue(value: T): void;
}

interface Pair<K, V> {
    key: K;
    value: V;
}

const numContainer: Container<number> = {
    value: 42,
    getValue() { return this.value; },
    setValue(v) { this.value = v; }
};

const pair: Pair<string, number> = {
    key: "age",
    value: 30
};

console.log(numContainer.getValue());
console.log(pair.key, pair.value);"#;

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
        output.contains("numContainer") || output.contains("pair"),
        "expected output to contain numContainer or pair. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic interface"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_generic() {
    // Test generic type alias declarations
    let source = r#"type Nullable<T> = T | null;
type Result<T, E> = { success: true; value: T } | { success: false; error: E };
type AsyncResult<T> = Promise<Result<T, Error>>;

const name: Nullable<string> = "John";
const nullName: Nullable<string> = null;

const success: Result<number, string> = { success: true, value: 42 };
const failure: Result<number, string> = { success: false, error: "Not found" };

function processResult<T>(result: Result<T, string>): T | null {
    if (result.success) {
        return result.value;
    }
    console.log("Error:", result.error);
    return null;
}

console.log(name, nullName);
console.log(processResult(success));
console.log(processResult(failure));"#;

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
        output.contains("processResult") || output.contains("success"),
        "expected output to contain processResult or success. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic type alias"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_mapped() {
    // Test mapped type aliases
    let source = r#"type Readonly<T> = { readonly [K in keyof T]: T[K] };
type Partial<T> = { [K in keyof T]?: T[K] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };

interface User {
    id: number;
    name: string;
    email: string;
}

const readonlyUser: Readonly<User> = {
    id: 1,
    name: "John",
    email: "john@example.com"
};

const partialUser: Partial<User> = {
    name: "Jane"
};

const pickedUser: Pick<User, "id" | "name"> = {
    id: 2,
    name: "Bob"
};

console.log(readonlyUser.name);
console.log(partialUser.name);
console.log(pickedUser.id, pickedUser.name);"#;

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
        output.contains("readonlyUser") || output.contains("partialUser"),
        "expected output to contain readonlyUser or partialUser. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mapped types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_type_alias_conditional() {
    // Test conditional type aliases
    let source = r#"type IsString<T> = T extends string ? true : false;
type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type NonNullable<T> = T extends null | undefined ? never : T;

type StringCheck = IsString<string>;
type NumberCheck = IsString<number>;

const value1: NonNullable<string | null> = "hello";
const value2: UnwrapPromise<Promise<number>> = 42;

function checkType<T>(value: T): IsString<T> {
    return (typeof value === "string") as any;
}

console.log(value1, value2);
console.log(checkType("test"));
console.log(checkType(123));"#;

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
        output.contains("checkType") || output.contains("value1"),
        "expected output to contain checkType or value1. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for conditional types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_type_alias_combined() {
    // Test combined interface and type alias patterns
    let source = r#"interface BaseEntity {
    id: number;
    createdAt: Date;
    updatedAt: Date;
}

interface User extends BaseEntity {
    username: string;
    email: string;
}

interface Post extends BaseEntity {
    title: string;
    content: string;
    authorId: number;
}

type EntityType = "user" | "post";
type Entity<T extends EntityType> = T extends "user" ? User : Post;

type CreateInput<T extends BaseEntity> = Omit<T, "id" | "createdAt" | "updatedAt">;
type UpdateInput<T extends BaseEntity> = Partial<CreateInput<T>>;

const userInput: CreateInput<User> = {
    username: "johndoe",
    email: "john@example.com"
};

const postUpdate: UpdateInput<Post> = {
    title: "Updated Title"
};

function createEntity<T extends EntityType>(
    type: T,
    input: CreateInput<Entity<T>>
): Entity<T> {
    const now = new Date();
    return {
        ...input,
        id: Math.random(),
        createdAt: now,
        updatedAt: now
    } as Entity<T>;
}

function updateEntity<T extends BaseEntity>(
    entity: T,
    updates: UpdateInput<T>
): T {
    return {
        ...entity,
        ...updates,
        updatedAt: new Date()
    };
}

console.log(userInput);
console.log(postUpdate);
console.log(createEntity("user", userInput));"#;

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
        output.contains("createEntity") || output.contains("updateEntity"),
        "expected output to contain createEntity or updateEntity. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined interface/type patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Additional Interface ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_interface_optional_properties() {
    // Test interface with optional properties
    let source = r#"interface Config {
    host: string;
    port?: number;
    secure?: boolean;
    timeout?: number;
}

const config: Config = {
    host: "localhost"
};

const fullConfig: Config = {
    host: "example.com",
    port: 443,
    secure: true,
    timeout: 5000
};

function createConnection(cfg: Config): void {
    console.log("Connecting to", cfg.host);
    if (cfg.port) {
        console.log("Port:", cfg.port);
    }
}

createConnection(config);
createConnection(fullConfig);"#;

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
        output.contains("config") && output.contains("createConnection"),
        "expected output to contain config and createConnection. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_readonly_properties() {
    // Test interface with readonly properties
    let source = r#"interface Point {
    readonly x: number;
    readonly y: number;
}

interface Circle {
    readonly center: Point;
    readonly radius: number;
}

const point: Point = { x: 10, y: 20 };
const circle: Circle = {
    center: { x: 0, y: 0 },
    radius: 5
};

function distance(p1: Point, p2: Point): number {
    const dx = p1.x - p2.x;
    const dy = p1.y - p2.y;
    return Math.sqrt(dx * dx + dy * dy);
}

console.log(point.x, point.y);
console.log(circle.center.x, circle.radius);
console.log(distance(point, circle.center));"#;

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
        output.contains("point") && output.contains("distance"),
        "expected output to contain point and distance. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for readonly properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_index_signature() {
    // Test interface with index signatures
    let source = r#"interface StringDictionary {
    [key: string]: string;
}

interface NumberDictionary {
    [index: number]: string;
    length: number;
}

interface MixedDictionary {
    [key: string]: number | string;
    name: string;
    count: number;
}

const dict: StringDictionary = {
    foo: "bar",
    hello: "world"
};

const numDict: NumberDictionary = {
    0: "first",
    1: "second",
    length: 2
};

const mixed: MixedDictionary = {
    name: "test",
    count: 42,
    extra: "value"
};

function getValues(d: StringDictionary): string[] {
    return Object.values(d);
}

console.log(dict["foo"]);
console.log(numDict[0]);
console.log(mixed.name, mixed.count);
console.log(getValues(dict));"#;

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
        output.contains("dict") && output.contains("getValues"),
        "expected output to contain dict and getValues. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for index signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_call_signature() {
    // Test interface with call signatures
    let source = r#"interface StringProcessor {
    (input: string): string;
}

interface Calculator {
    (a: number, b: number): number;
    description: string;
}

interface Formatter {
    (value: any): string;
    (value: any, format: string): string;
}

const uppercase: StringProcessor = function(input) {
    return input.toUpperCase();
};

const add: Calculator = function(a, b) {
    return a + b;
};
add.description = "Adds two numbers";

const format: Formatter = function(value: any, fmt?: string) {
    if (fmt) {
        return fmt + ": " + String(value);
    }
    return String(value);
};

console.log(uppercase("hello"));
console.log(add(5, 3), add.description);
console.log(format(42));
console.log(format(42, "Number"));"#;

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
        output.contains("uppercase") && output.contains("format"),
        "expected output to contain uppercase and format. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for call signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_construct_signature() {
    // Test interface with construct signatures
    let source = r#"interface PointConstructor {
    new(x: number, y: number): { x: number; y: number };
}

interface ClockConstructor {
    new(hour: number, minute: number): ClockInterface;
}

interface ClockInterface {
    tick(): void;
    getTime(): string;
}

function createPoint(ctor: PointConstructor, x: number, y: number) {
    return new ctor(x, y);
}

const PointClass: PointConstructor = class {
    constructor(public x: number, public y: number) {}
};

const point = createPoint(PointClass, 10, 20);
console.log(point.x, point.y);"#;

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
        output.contains("createPoint") || output.contains("PointClass"),
        "expected output to contain createPoint or PointClass. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for construct signature"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_interface_merging() {
    // Test interface merging (declaration merging)
    let source = r#"interface Box {
    height: number;
    width: number;
}

interface Box {
    depth: number;
    color: string;
}

interface Box {
    weight?: number;
}

const box: Box = {
    height: 10,
    width: 20,
    depth: 30,
    color: "red"
};

const heavyBox: Box = {
    height: 5,
    width: 5,
    depth: 5,
    color: "blue",
    weight: 100
};

function describeBox(b: Box): string {
    let desc = b.color + " box: " + b.width + "x" + b.height + "x" + b.depth;
    if (b.weight) {
        desc += " (" + b.weight + "kg)";
    }
    return desc;
}

console.log(describeBox(box));
console.log(describeBox(heavyBox));"#;

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
        output.contains("box") && output.contains("describeBox"),
        "expected output to contain box and describeBox. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface merging"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

