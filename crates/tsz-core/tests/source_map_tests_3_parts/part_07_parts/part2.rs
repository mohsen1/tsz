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

#[test]
fn test_source_map_private_field_es5_static_field_access() {
    let source = r#"class IdGenerator {
    static #nextId: number = 1;
    static #prefix: string = "ID_";

    static generate(): string {
        return IdGenerator.#prefix + IdGenerator.#nextId++;
    }

    static reset(): void {
        IdGenerator.#nextId = 1;
    }

    static setPrefix(prefix: string): void {
        IdGenerator.#prefix = prefix;
    }

    static getNextId(): number {
        return IdGenerator.#nextId;
    }
}

console.log(IdGenerator.generate());
console.log(IdGenerator.generate());
IdGenerator.setPrefix("USER_");
console.log(IdGenerator.generate());
console.log(IdGenerator.getNextId());"#;

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
        output.contains("IdGenerator"),
        "expected output to contain IdGenerator class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private static field access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
