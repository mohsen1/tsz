#[test]
fn test_source_map_class_inheritance_es5_super_calls() {
    let source = r#"class Base {
    protected value: number;

    constructor(value: number) {
        this.value = value;
    }

    getValue(): number {
        return this.value;
    }

    protected increment(): void {
        this.value++;
    }
}

class Derived extends Base {
    private multiplier: number;

    constructor(value: number, multiplier: number) {
        super(value);
        this.multiplier = multiplier;
    }

    getValue(): number {
        return super.getValue() * this.multiplier;
    }

    increment(): void {
        super.increment();
        console.log("Incremented to", this.value);
    }
}

const d = new Derived(5, 2);
console.log(d.getValue());
d.increment();"#;

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
        output.contains("Derived"),
        "expected output to contain Derived class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for super calls"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_method_overrides() {
    let source = r#"class Shape {
    protected x: number;
    protected y: number;

    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }

    area(): number {
        return 0;
    }

    perimeter(): number {
        return 0;
    }

    describe(): string {
        return `Shape at (${this.x}, ${this.y})`;
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

    area(): number {
        return this.width * this.height;
    }

    perimeter(): number {
        return 2 * (this.width + this.height);
    }

    describe(): string {
        return `Rectangle ${this.width}x${this.height} at (${this.x}, ${this.y})`;
    }
}

class Circle extends Shape {
    private radius: number;

    constructor(x: number, y: number, radius: number) {
        super(x, y);
        this.radius = radius;
    }

    area(): number {
        return Math.PI * this.radius * this.radius;
    }

    perimeter(): number {
        return 2 * Math.PI * this.radius;
    }

    describe(): string {
        return `Circle r=${this.radius} at (${this.x}, ${this.y})`;
    }
}

const shapes: Shape[] = [new Rectangle(0, 0, 10, 5), new Circle(5, 5, 3)];
shapes.forEach(s => console.log(s.describe(), s.area()));"#;

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
        output.contains("Rectangle"),
        "expected output to contain Rectangle class. output: {output}"
    );
    assert!(
        output.contains("Circle"),
        "expected output to contain Circle class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method overrides"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_multi_level() {
    let source = r#"class Entity {
    id: string;

    constructor(id: string) {
        this.id = id;
    }

    toString(): string {
        return `Entity(${this.id})`;
    }
}

class LivingEntity extends Entity {
    health: number;

    constructor(id: string, health: number) {
        super(id);
        this.health = health;
    }

    isAlive(): boolean {
        return this.health > 0;
    }

    toString(): string {
        return `${super.toString()} HP:${this.health}`;
    }
}

class Character extends LivingEntity {
    name: string;
    level: number;

    constructor(id: string, name: string, health: number, level: number) {
        super(id, health);
        this.name = name;
        this.level = level;
    }

    toString(): string {
        return `${this.name} Lv.${this.level} ${super.toString()}`;
    }
}

class Player extends Character {
    experience: number;

    constructor(id: string, name: string) {
        super(id, name, 100, 1);
        this.experience = 0;
    }

    gainExp(amount: number): void {
        this.experience += amount;
        if (this.experience >= this.level * 100) {
            this.level++;
            this.health += 10;
        }
    }

    toString(): string {
        return `[Player] ${super.toString()} EXP:${this.experience}`;
    }
}

const player = new Player("p1", "Hero");
player.gainExp(150);
console.log(player.toString());"#;

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
        output.contains("Player"),
        "expected output to contain Player class. output: {output}"
    );
    assert!(
        output.contains("Character"),
        "expected output to contain Character class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multi-level inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_mixin_pattern() {
    let source = r#"type Constructor<T = {}> = new (...args: any[]) => T;

function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        timestamp = Date.now();

        getTimestamp(): number {
            return this.timestamp;
        }
    };
}

function Tagged<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        tag: string = "";

        setTag(tag: string): void {
            this.tag = tag;
        }

        getTag(): string {
            return this.tag;
        }
    };
}

function Serializable<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        serialize(): string {
            return JSON.stringify(this);
        }
    };
}

class BaseEntity {
    id: number;

    constructor(id: number) {
        this.id = id;
    }
}

const MixedEntity = Serializable(Tagged(Timestamped(BaseEntity)));

const entity = new MixedEntity(1);
entity.setTag("important");
console.log(entity.serialize());"#;

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
        output.contains("Timestamped"),
        "expected output to contain Timestamped mixin. output: {output}"
    );
    assert!(
        output.contains("Tagged"),
        "expected output to contain Tagged mixin. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mixin pattern"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_super_property_access() {
    let source = r#"class Config {
    protected settings: Map<string, any> = new Map();

    get(key: string): any {
        return this.settings.get(key);
    }

    set(key: string, value: any): void {
        this.settings.set(key, value);
    }

    has(key: string): boolean {
        return this.settings.has(key);
    }
}

class AppConfig extends Config {
    private defaults: Map<string, any>;

    constructor(defaults: Record<string, any>) {
        super();
        this.defaults = new Map(Object.entries(defaults));
    }

    get(key: string): any {
        if (super.has(key)) {
            return super.get(key);
        }
        return this.defaults.get(key);
    }

    set(key: string, value: any): void {
        if (this.defaults.has(key)) {
            super.set(key, value);
        } else {
            throw new Error(`Unknown config key: ${key}`);
        }
    }

    reset(key: string): void {
        if (this.defaults.has(key)) {
            super.set(key, this.defaults.get(key));
        }
    }
}

const config = new AppConfig({ debug: false, timeout: 5000 });
config.set("debug", true);
console.log(config.get("debug"), config.get("timeout"));"#;

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
        output.contains("AppConfig"),
        "expected output to contain AppConfig class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for super property access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_static_inheritance() {
    let source = r#"class Database {
    static connectionCount = 0;
    static instances: Database[] = [];

    static create(): Database {
        const db = new Database();
        Database.instances.push(db);
        return db;
    }

    static getConnectionCount(): number {
        return Database.connectionCount;
    }

    constructor() {
        Database.connectionCount++;
    }
}

class PostgresDB extends Database {
    static driver = "pg";

    static create(): PostgresDB {
        const db = new PostgresDB();
        Database.instances.push(db);
        return db;
    }

    static getDriver(): string {
        return PostgresDB.driver;
    }

    query(sql: string): void {
        console.log(`Executing on ${PostgresDB.driver}: ${sql}`);
    }
}

class MySQLDB extends Database {
    static driver = "mysql2";

    static create(): MySQLDB {
        const db = new MySQLDB();
        Database.instances.push(db);
        return db;
    }
}

const pg = PostgresDB.create();
const mysql = MySQLDB.create();
console.log(Database.getConnectionCount());"#;

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
        output.contains("PostgresDB"),
        "expected output to contain PostgresDB class. output: {output}"
    );
    assert!(
        output.contains("MySQLDB"),
        "expected output to contain MySQLDB class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_abstract_class() {
    let source = r#"abstract class Transport {
    abstract connect(): Promise<void>;
    abstract disconnect(): Promise<void>;
    abstract send(data: string): Promise<void>;

    protected connected = false;

    isConnected(): boolean {
        return this.connected;
    }

    async sendIfConnected(data: string): Promise<boolean> {
        if (this.connected) {
            await this.send(data);
            return true;
        }
        return false;
    }
}

class WebSocketTransport extends Transport {
    private url: string;
    private ws: any;

    constructor(url: string) {
        super();
        this.url = url;
    }

    async connect(): Promise<void> {
        this.ws = new WebSocket(this.url);
        this.connected = true;
    }

    async disconnect(): Promise<void> {
        this.ws?.close();
        this.connected = false;
    }

    async send(data: string): Promise<void> {
        this.ws?.send(data);
    }
}

class HTTPTransport extends Transport {
    private baseUrl: string;

    constructor(baseUrl: string) {
        super();
        this.baseUrl = baseUrl;
    }

    async connect(): Promise<void> {
        this.connected = true;
    }

    async disconnect(): Promise<void> {
        this.connected = false;
    }

    async send(data: string): Promise<void> {
        await fetch(this.baseUrl, { method: 'POST', body: data });
    }
}

const transports: Transport[] = [
    new WebSocketTransport("ws://localhost:8080"),
    new HTTPTransport("http://api.example.com")
];"#;

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
        output.contains("Transport"),
        "expected output to contain Transport class. output: {output}"
    );
    assert!(
        output.contains("WebSocketTransport"),
        "expected output to contain WebSocketTransport class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for abstract class"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_interface_implementation() {
    let source = r#"interface Comparable<T> {
    compareTo(other: T): number;
}

interface Hashable {
    hashCode(): number;
}

interface Cloneable<T> {
    clone(): T;
}

class BaseValue {
    protected value: number;

    constructor(value: number) {
        this.value = value;
    }

    getValue(): number {
        return this.value;
    }
}

class ComparableValue extends BaseValue implements Comparable<ComparableValue>, Hashable, Cloneable<ComparableValue> {
    constructor(value: number) {
        super(value);
    }

    compareTo(other: ComparableValue): number {
        return this.value - other.value;
    }

    hashCode(): number {
        return this.value | 0;
    }

    clone(): ComparableValue {
        return new ComparableValue(this.value);
    }
}

const a = new ComparableValue(10);
const b = new ComparableValue(20);
console.log(a.compareTo(b));
console.log(a.hashCode());
const c = a.clone();"#;

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
        output.contains("ComparableValue"),
        "expected output to contain ComparableValue class. output: {output}"
    );
    assert!(
        output.contains("BaseValue"),
        "expected output to contain BaseValue class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for interface implementation"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_inheritance_es5_comprehensive() {
    let source = r#"// Comprehensive class inheritance test with mixins, abstract classes, and interfaces

type Constructor<T = {}> = new (...args: any[]) => T;

interface Identifiable {
    getId(): string;
}

interface Persistable {
    save(): Promise<void>;
    load(): Promise<void>;
}

function Loggable<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        log(message: string): void {
            console.log(`[${new Date().toISOString()}] ${message}`);
        }
    };
}

function Validatable<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        protected errors: string[] = [];

        validate(): boolean {
            this.errors = [];
            return true;
        }

        getErrors(): string[] {
            return [...this.errors];
        }
    };
}

abstract class Entity implements Identifiable {
    protected id: string;
    protected createdAt: Date;
    protected updatedAt: Date;

    constructor(id?: string) {
        this.id = id || crypto.randomUUID();
        this.createdAt = new Date();
        this.updatedAt = new Date();
    }

    getId(): string {
        return this.id;
    }

    abstract toJSON(): object;
}

abstract class Model extends Entity implements Persistable {
    protected dirty = false;

    markDirty(): void {
        this.dirty = true;
        this.updatedAt = new Date();
    }

    abstract save(): Promise<void>;
    abstract load(): Promise<void>;
}

const ValidatableModel = Validatable(Loggable(class extends Model {
    toJSON(): object {
        return { id: this.id, createdAt: this.createdAt, updatedAt: this.updatedAt };
    }

    async save(): Promise<void> {
        this.log(`Saving entity ${this.id}`);
    }

    async load(): Promise<void> {
        this.log(`Loading entity ${this.id}`);
    }
}));

class User extends ValidatableModel {
    private email: string;
    private name: string;
    private role: "admin" | "user" | "guest";

    constructor(email: string, name: string, role: "admin" | "user" | "guest" = "user") {
        super();
        this.email = email;
        this.name = name;
        this.role = role;
    }

    validate(): boolean {
        super.validate();

        if (!this.email.includes("@")) {
            this.errors.push("Invalid email format");
        }
        if (this.name.length < 2) {
            this.errors.push("Name too short");
        }

        return this.errors.length === 0;
    }

    toJSON(): object {
        return {
            ...super.toJSON(),
            email: this.email,
            name: this.name,
            role: this.role
        };
    }

    async save(): Promise<void> {
        if (!this.validate()) {
            throw new Error(`Validation failed: ${this.getErrors().join(", ")}`);
        }
        await super.save();
        this.dirty = false;
    }

    promote(): void {
        if (this.role === "guest") {
            this.role = "user";
        } else if (this.role === "user") {
            this.role = "admin";
        }
        this.markDirty();
    }
}

class AdminUser extends User {
    private permissions: Set<string>;

    constructor(email: string, name: string, permissions: string[] = []) {
        super(email, name, "admin");
        this.permissions = new Set(permissions);
    }

    hasPermission(permission: string): boolean {
        return this.permissions.has(permission) || this.permissions.has("*");
    }

    grant(permission: string): void {
        this.permissions.add(permission);
        this.markDirty();
    }

    revoke(permission: string): void {
        this.permissions.delete(permission);
        this.markDirty();
    }

    toJSON(): object {
        return {
            ...super.toJSON(),
            permissions: [...this.permissions]
        };
    }
}

// Usage
const admin = new AdminUser("admin@example.com", "Admin", ["users.read", "users.write"]);
admin.grant("settings.read");
admin.validate();
console.log(JSON.stringify(admin.toJSON(), null, 2));

const user = new User("test", "A", "guest");
if (!user.validate()) {
    console.log("Validation errors:", user.getErrors());
}
user.promote();
console.log(JSON.stringify(user.toJSON(), null, 2));"#;

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
        output.contains("User"),
        "expected output to contain User class. output: {output}"
    );
    assert!(
        output.contains("AdminUser"),
        "expected output to contain AdminUser class. output: {output}"
    );
    assert!(
        output.contains("Entity"),
        "expected output to contain Entity class. output: {output}"
    );
    assert!(
        output.contains("Loggable"),
        "expected output to contain Loggable mixin. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive class inheritance"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Private Field ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_private_field_es5_instance_field_access() {
    let source = r#"class Counter {
    #count: number = 0;

    increment(): void {
        this.#count++;
    }

    decrement(): void {
        this.#count--;
    }

    getCount(): number {
        return this.#count;
    }

    setCount(value: number): void {
        this.#count = value;
    }

    reset(): void {
        this.#count = 0;
    }
}

const counter = new Counter();
counter.increment();
counter.increment();
console.log(counter.getCount());
counter.setCount(10);
console.log(counter.getCount());"#;

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
        output.contains("Counter"),
        "expected output to contain Counter class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private instance field access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

